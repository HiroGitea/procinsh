//! Unprivileged ELF parsing, caching, address conversion and source resolution.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`ElfImage`] | `pub` | `struct ElfImage` |
//! | [`ElfCache`] | `pub` | `struct ElfCache` |
//! | [`ElfFileId`] | `pub` | `type ElfFileId = (u64, u64, u64, i64, i64)` |
//! | [`ElfSymbols`] | `pub` | `struct ElfSymbols` |
//! | [`SymbolInfo`] | `pub` | `struct SymbolInfo` |
//! | [`elf_address`] | `pub` | `fn elf_address(address: u64, map: &MemoryMap, elf: &ElfSymbols, page: u64) -> Option<u64>` |
//! | [`instruction_address`] | `pub` | `fn instruction_address(address: u64, return_address: bool) -> u64` |
//! | [`resolve_frame`] | `pub` | `fn resolve_frame(address: u64, elf: &ElfSymbols) -> Option<SymbolInfo>` |
//!
//! Mapping arguments use [`MemoryMap`](crate::model::memory::MemoryMap).

mod cache;
mod image;
pub use image::ElfImage;
mod resolve;
#[cfg(test)]
pub use cache::ElfFile;
pub use cache::{ElfCache, ElfFileId, ElfSymbols};
pub use resolve::{SymbolInfo, elf_address, instruction_address, resolve_frame};
