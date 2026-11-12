use std::{path::PathBuf, sync::Arc, time::{Duration, Instant}};

use moka::{future::Cache, notification::RemovalCause};
use tokio::sync::Semaphore;
use wasmtime::{
    Config, Engine, InstanceAllocationStrategy, OptLevel, PoolingAllocationConfig, ProfilingStrategy, component::{Component, Linker},
};
use wasmtime_wasi_http::p2::bindings::ProxyPre;

use crate::{EPOCH_TICK, IDLE_TIMEOUT, Result, TenantState, db::{DatabaseManager, TenantMetadata}, error::AppError};

#[derive(Clone)]
pub struct CompiledTenant {
    // Lưu thông tin tenant vào cache
    pub meta: TenantMetadata,
    pub pre: ProxyPre<TenantState>,
    // Kính thước mã máy sau khi compile
    pub compiled_bytes: usize
}

#[derive(Clone)]
pub struct TenantRuntime {
    pub engine: Engine,
    linker: Arc<Linker<TenantState>>,
    compiled: Cache<String, CompiledTenant>,
    // Giới hạn số compile chạy đồng thời
    compile_limit: Arc<Semaphore>
}

impl TenantRuntime {
    pub fn new() -> Result<TenantRuntime> {
        let engine = build_engine()?;
        let linker = Arc::new(build_linker(&engine)?);
        let compiled= Cache::builder()
                    .max_capacity(500)
                    .time_to_idle(Duration::from_secs(IDLE_TIMEOUT))
                    .eviction_listener(|tenant_id: Arc<String>, v: CompiledTenant, cause: RemovalCause|{
                        let mb = v.compiled_bytes as f64 / 1024.0 / 1024.0;
                        match cause {
                            RemovalCause::Expired => tracing::info!(tenant_id = %tenant_id,"Giải phóng {mb:.2} MB khỏi RAM"),
                            RemovalCause::Explicit => tracing::info!(tenant_id = %tenant_id,"Tenant upload bản wasm mới"),
                            RemovalCause::Replaced => tracing::debug!(tenant_id = %tenant_id,"Bản compile được thay thế"),
                            RemovalCause::Size => tracing::info!(tenant_id = %tenant_id,"Cache đầy → loại tenant, giải phóng {mb:.2} MB"),
                        }
                    })
                    .build();
        let maintenance = compiled.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(30));
            loop {
                tick.tick().await;
                maintenance.run_pending_tasks().await;
            }
        });

        let permits = std::thread::available_parallelism()
                .map(|n| (n.get() / 2).max(1))
                .unwrap_or(1);

        Ok(TenantRuntime { engine, linker, compiled , compile_limit: Arc::new(Semaphore::new(permits))})
    }

    pub async fn get(&self, tenant_id: &str, db: &DatabaseManager) -> Result<CompiledTenant> {
        let this = self.clone();
        let db = db.clone();
        let id = tenant_id.to_owned();

        self.compiled
            .try_get_with(tenant_id.to_owned(), async move {
                // Tra metadata + lấy ProxyPre đã compile
                let meta = db
                    .get_tenant(&id)
                    .await?
                    .ok_or(AppError::NotFound("Tenant không tồn tại"))?;

                let component = this.load_component(&meta).await?;
                let range = component.image_range();
                let compiled_bytes = range.end as usize - range.start as usize;
                let pre = ProxyPre::new(this.linker.instantiate_pre(&component)?)?;

                Ok::<_, AppError>(CompiledTenant { meta, pre, compiled_bytes })
        }).await
        .map_err(|e| match &*e {
            AppError::NotFound(m) => AppError::NotFound(m),
            _ => AppError::Internal(anyhow::anyhow!("{e}")),
        })
    }

    /// Ưu tiên nạp file .cwasm, không có -> compile từ wasm rồi lưu lại
    async fn load_component(&self, meta: &TenantMetadata) -> Result<Component> {
        let _permit = self.compile_limit.acquire().await.map_err(|e| AppError::Internal(e.into()))?;

        let engine = self.engine.clone();
        let wasm_path = PathBuf::from(&meta.wasm_path);
        let cwasm_path = wasm_path.with_extension("cwasm");
        let id = meta.tenant_id.clone();
        let started = Instant::now();

        let (component, from_cache) = tokio::task::spawn_blocking(move || -> Result<(Component, bool)> {
            if cwasm_path.exists() {
                // SAFETY: file .cwasm chỉ do chính gateway tạo ra (lúc upload hoặc bên dưới)
                match unsafe { Component::deserialize_file(&engine, &cwasm_path) } {
                    Ok(c) => return Ok((c, true)),
                    // Thường do đã nâng cấp wasmtime hoặc đổi Config → compile lại
                    Err(e) => tracing::warn!(error = %e, "Không dùng được .cwasm → compile lại"),
                }
            }
            let bytes = std::fs::read(&wasm_path)?;
            let c = Component::new(&engine, &bytes)?;
            // Lưu lại cho lần sau (ghi file tạm rồi rename để không bị đọc dở)
            if let Ok(ser) = c.serialize() {
                let tmp = cwasm_path.with_extension("cwasm.tmp");
                if std::fs::write(&tmp, ser).is_ok() {
                    let _ = std::fs::rename(&tmp, &cwasm_path);
                }
            }
            Ok((c, false))
        })
        .await
        .map_err(|e| AppError::Internal(e.into()))??;

        tracing::info!(
            tenant_id = %id,
            ms = started.elapsed().as_millis() as u64,
            source = if from_cache { "cwasm" } else { "compile" },
            "Nạp tenant vào RAM"
        );
        Ok(component)
    }

    // pub async fn proxy_pre(&self, tenant_id: &str, db: &DatabaseManager) -> Result<ProxyPre<TenantState>>{
    //     let engine = self.engine.clone();
    //     let linker = self.linker.clone();
    //     let path = meta.wasm_path.clone();

    //     self.compiled.try_get_with(meta.tenant_id.clone(), async move {
    //         let bytes = tokio::fs::read(&path).await?;
    //         let component = tokio::task::spawn_blocking(move || Component::new(&engine, &bytes))
    //                         .await
    //                         .map_err(|e| AppError::Internal(e.into()))??;
    //         Ok::<_, AppError>(ProxyPre::new(linker.instantiate_pre(&component)?)?)
    //     })
    //     .await
    //     .map_err(|e| AppError::Internal(e.into()))
    // }

    pub async fn invalidate(&self, tenant_id: &str) {
        self.compiled.invalidate(tenant_id).await
    }
}

pub fn build_engine() -> Result<Engine> {
    let mut config = Config::new();

    config.cranelift_opt_level(OptLevel::Speed);
    config.wasm_component_model_async(true);
    config.epoch_interruption(true);

    let max_instances: u32 = std::env::var("MAX_INSTANCES").ok().and_then(|v| v.parse().ok()).unwrap_or(1000);
    let mut pool = PoolingAllocationConfig::default();
    pool.total_component_instances(max_instances);
    pool.max_memory_size(1024 * 64 * 1024);
    config.allocation_strategy(InstanceAllocationStrategy::Pooling(pool));

    config.profiler(ProfilingStrategy::None);

    let engine = Engine::new(&config)?;
    let ticker = engine.clone();
    std::thread::Builder::new()
        .name("wasm-epoch-ticker".into())
        .spawn(move || {
            loop {
                std::thread::sleep(EPOCH_TICK);
                ticker.increment_epoch();
            }
        })?;
    Ok(engine)
}

pub fn build_linker(engine: &Engine) ->  Result<Linker<TenantState>> {
    let mut linker = Linker::new(engine);
    wasmtime_wasi::p3::add_to_linker(&mut linker)?;
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    wasmtime_wasi_http::p2::add_only_http_to_linker_async(&mut linker)?;

    Ok(linker)
}