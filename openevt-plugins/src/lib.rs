#[global_allocator]
static GLOBAL: hotpath::CountingAllocator = hotpath::CountingAllocator::new();

pub mod simulator;

#[cfg(feature = "python")]
mod python;
