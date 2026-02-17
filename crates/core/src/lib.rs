#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(rustdoc::broken_intra_doc_links)]

//! Core library for the Beeno CLI.
//!
//! `beeno_core` provides:
//! - translation orchestration via [`engine`]
//! - provider adapters via [`providers`]
//! - interactive shell flows via [`repl`]
//! - background server management via [`server`]
//! - shared configuration and request/response types via [`types`]
//!
//! # Quick Start
//!
//! ```no_run
//! use beeno_core::engine::{DefaultRiskPolicy, Engine};
//! use beeno_core::providers::MockProvider;
//! use beeno_core::types::SessionSummary;
//!
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let engine = Engine::new(MockProvider, DefaultRiskPolicy::default());
//! let (source, _translated, _risk) = engine
//!     .prepare_source(
//!         "print hello from beeno",
//!         "eval",
//!         SessionSummary::default(),
//!         None,
//!     )
//!     .await?;
//! assert!(source.contains("console.log"));
//! # Ok(())
//! # }
//! ```

pub mod engine;
pub mod providers;
pub mod repl;
pub mod server;
pub mod types;
