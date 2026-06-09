pub mod heightmap;
pub mod simulator;

#[allow(unused_imports)]
pub use heightmap::{Gradient, Heightmap};
#[allow(unused_imports)]
pub use simulator::{
    displace_into, gaussian_blur, gaussian_kernel_1d, sample_downhill_delta, simple_gradient,
    simple_gradient_into, ErosionConfig, ErosionSimulator,
};
