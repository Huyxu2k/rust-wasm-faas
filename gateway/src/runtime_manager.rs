
use wasmtime::{
    Config, Engine, InstanceAllocationStrategy, OptLevel, PoolingAllocationConfig,
    ProfilingStrategy,
};

use crate::Result;

pub fn create_engine() -> Result<Engine> {
    let mut config = Config::new();

    config.cranelift_opt_level(OptLevel::Speed);

    config.epoch_interruption(true);

    let mut pool = PoolingAllocationConfig::default();
    pool.total_component_instances(1000);         
    pool.max_memory_size(1024 * 64 * 1024);     
    config.allocation_strategy(InstanceAllocationStrategy::Pooling(pool));

    config.profiler(ProfilingStrategy::None);

    Ok(Engine::new(&config)?)
}

//  let e = engine.clone();
//   std::thread::spawn(move || loop {
//       std::thread::sleep(std::time::Duration::from_millis(10));
//       e.increment_epoch();
//   });