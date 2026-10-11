//! Sleeper power rankings: pull a league from the Sleeper API and build a
//! static, mobile-first site from it.
//!
//! Layout mirrors the pipeline: `sleeper` fetches raw responses, `compute`
//! turns them into `models` (pure functions), `build` renders `site/` and
//! `pipeline` ties the two together for the CLI.

pub mod build;
pub mod compute;
pub mod config;
pub mod models;
pub mod pipeline;
pub mod pyjson;
pub mod pyrand;
pub mod raw;
pub mod serve;
pub mod sleeper;
pub mod util;
