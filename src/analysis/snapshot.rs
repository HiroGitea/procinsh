//! Interpret captured registers, stack bytes and verified ELF bytes in main.
//!
//! # Interface
//!
//! | Definition | Visibility | Kind / signature |
//! | --- | --- | --- |
//! | [`registers`] | `pub` | `mod registers` |
//! | [`stack`] | `pub` | `mod stack` |
//! | [`symbol`] | `pub` | `mod symbol` |
//! | [`unwind`] | `pub` | `mod unwind` |

pub mod registers;
pub mod stack;
pub mod symbol;
pub mod unwind;
