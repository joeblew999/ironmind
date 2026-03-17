use crate::config::ModelConfig;
use anyhow::Result;

#[cfg(feature = "inference")]
pub struct IronMindModel {
    pub inner: mistralrs::Model,
}

#[cfg(not(feature = "inference"))]
pub struct IronMindModel;

impl IronMindModel {
    #[cfg(feature = "inference")]
    pub async fn load(cfg: &ModelConfig) -> Result<Self> {
        use mistralrs::{IsqType, TextModelBuilder};
        use std::str::FromStr;

        std::env::set_var("HF_HUB_OFFLINE", "1");
        std::env::set_var(
            "HF_HUB_CACHE",
            cfg.weights_path.parent().unwrap_or(&cfg.weights_path),
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

    #[cfg(not(feature = "inference"))]
    pub async fn load(_cfg: &ModelConfig) -> Result<Self> {
        anyhow::bail!(
            "ironmind compiled without inference support. \
             Rebuild with --features metal (Apple Silicon) or --features cuda."
        )
    }
}
