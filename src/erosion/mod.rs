pub mod heightmap;
pub mod simulator;

#[allow(unused_imports)]
pub use heightmap::{Gradient, Heightmap};
#[allow(unused_imports)]
pub use simulator::{
    displace, gaussian_blur, gaussian_kernel_1d, sample, simple_gradient, ErosionConfig,
    ErosionSimulator,
};
