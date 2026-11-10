use std::{sync::Arc, time::Duration};

use moka::future::Cache;
use wasmtime::{
    Config, Engine, InstanceAllocationStrategy, OptLevel, PoolingAllocationConfig, ProfilingStrategy, component::{Component, Linker},
};
use wasmtime_wasi_http::p2::bindings::ProxyPre;

use crate::{EPOCH_TICK, Result, TenantState, db::TenantMetadata, error::AppError};

#[derive(Clone)]
pub struct TenantRuntime {
    pub engine: Engine,
    linker: Arc<Linker<TenantState>>,
    compiled: Cache<String, ProxyPre<TenantState>>,
}

impl TenantRuntime {
    pub fn new() -> Result<TenantRuntime> {
        let engine = build_engine()?;
        let linker = Arc::new(build_linker(&engine)?);
        let compiled= Cache::builder()
                    .max_capacity(500)
                    .time_to_idle(Duration::from_secs(15*60))
                    .build();
        Ok(TenantRuntime { engine, linker, compiled })

    }

    pub async fn proxy_pre(&self, meta: &TenantMetadata) -> Result<ProxyPre<TenantState>>{
        let engine = self.engine.clone();
        let linker = self.linker.clone();
        let path = meta.wasm_path.clone();

        self.compiled.try_get_with(meta.tenant_id.clone(), async move {
            let bytes = tokio::fs::read(&path).await?;
            let component = tokio::task::spawn_blocking(move || Component::new(&engine, &bytes))
                            .await
                            .map_err(|e| AppError::Internal(e.into()))??;
            Ok::<_, AppError>(ProxyPre::new(linker.instantiate_pre(&component)?)?)
        })
        .await
        .map_err(|e| AppError::Internal(e.into()))
    }

    pub async fn invalidate(&self, tenant_id: &str) {
        self.compiled.invalidate(tenant_id).await
    }
}

pub fn build_engine() -> Result<Engine> {
    let mut config = Config::new();

    config.cranelift_opt_level(OptLevel::Speed);
    config.wasm_component_model_async(true);
    config.epoch_interruption(true);

    let mut pool = PoolingAllocationConfig::default();
    pool.total_component_instances(1000);
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