//! GPUs, RDMA NICs, accelerators and NVSwitches with the PCIe bridges between them,
//! the paths between devices, and the processes holding their character devices.
//!
//! Everything comes from sysfs and procfs without privileges; `config` space and
//! vendor libraries are not used, so no device is woken or modified.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`HardwareDevice`] | `pub` | `struct HardwareDevice` |
//! | [`DeviceHandle`] | `pub` | `struct DeviceHandle` |
//! | [`HardwareObservation`] | `pub` | `struct HardwareObservation` |
//! | [`HardwareSources`] | `pub` | `struct HardwareSources` |
//! | [`HardwarePath`] | `pub` | `struct HardwarePath` |

mod devices;
mod handles;
mod observation;
mod paths;
mod pci;
pub use devices::HardwareDevice;
pub use handles::DeviceHandle;
pub use observation::{HardwareObservation, HardwareSources};
pub use paths::HardwarePath;
