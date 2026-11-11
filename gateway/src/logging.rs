use tracing::Level;
use tracing_subscriber::FmtSubscriber;

pub fn init_logging(){
    let subscriber = FmtSubscriber::builder()
                .with_max_level(Level::TRACE)
                .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to setup logging!")
}