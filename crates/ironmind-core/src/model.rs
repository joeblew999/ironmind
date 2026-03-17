use crate::config::ModelConfig;
use anyhow::Result;
use mistralrs::{IsqType, TextModelBuilder};
use std::str::FromStr;

pub struct IronMindModel {
    pub inner: mistralrs::Model,
}

impl IronMindModel {
    /// Load model from local weights path — fully offline, no HF hub call.
    pub async fn load(cfg: &ModelConfig) -> Result<Self> {
        // Belt-and-braces: disable HF hub at env level for factory deploys
        std::env::set_var("HF_HUB_OFFLINE", "1");
        std::env::set_var(
            "HF_HUB_CACHE",
            cfg.weights_path
                .parent()
                .unwrap_or(&cfg.weights_path),
        );

        let isq = IsqType::from_str(&cfg.isq)
            .map_err(|e| anyhow::anyhow!("Invalid ISQ type '{}': {}", cfg.isq, e))?;

        let model = TextModelBuilder::new(
            cfg.weights_path
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid weights path"))?,
        )
        .with_isq(isq)
        .with_logging()
        .build()
        .await?;

        Ok(Self { inner: model })
    }
}
