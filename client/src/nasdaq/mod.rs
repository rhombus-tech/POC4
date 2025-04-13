/*!
 * NASDAQ integration module
 * 
 * This module provides specialized integration with NASDAQ's Capital Access Platform.
 * Currently implemented with mock interfaces to support the overall architecture,
 * it will be replaced with actual implementation when the NASDAQ API is available.
 */

mod client;
mod types;

pub use client::NasdaqClient;
pub use types::*;
