/*!
 * Adapters for external API integration
 * 
 * This module provides a flexible adapter layer for connecting external APIs
 * to the Aristo TEE mesh network. It's designed to be extended with specific
 * implementations for APIs like NASDAQ's Capital Access Platform.
 */

mod types;
mod api_adapter;

pub use types::*;
pub use api_adapter::ApiAdapter;
