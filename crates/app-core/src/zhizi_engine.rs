//! Fixed, public Zhizi choices. No model discovery or private server access.
use crate::{EngineProfile, RemoteEngineConfig};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ZhiziPlan {
    #[default]
    Vip,
    OnDemand1x,
    OnDemand3x,
    OnDemand6x,
    OnDemand12x,
    OnDemand24x,
    OnDemand1xCuda,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ZhiziWeight {
    #[serde(rename = "20b")]
    B20,
    #[serde(rename = "10b384t")]
    Transformer10b384,
    #[default]
    #[serde(rename = "10b512t")]
    Transformer10b512,
    #[serde(rename = "11b768t")]
    Transformer11b768,
    #[serde(rename = "18bnbt")]
    B18Nbt,
    #[serde(rename = "28bnbt")]
    B28Nbt,
    #[serde(rename = "fdx")]
    Fdx,
    #[serde(rename = "60b")]
    B60,
    #[serde(rename = "40b")]
    B40,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct ZhiziEngineOptions {
    #[ts(as = "Option<ZhiziPlan>", optional)]
    pub plan: ZhiziPlan,
    #[ts(as = "Option<ZhiziWeight>", optional)]
    pub weight: ZhiziWeight,
}

impl ZhiziEngineOptions {
    /// Values come from RemoteComputeConfig.java:32–46 and
    /// ZhiziEngineCatalog.java:22–33; arbitrary shell arguments are not accepted.
    pub fn connection_args(&self) -> String {
        let gpu = match self.plan {
            ZhiziPlan::Vip => "vip-share",
            ZhiziPlan::OnDemand1x | ZhiziPlan::OnDemand1xCuda => "1x",
            ZhiziPlan::OnDemand3x => "3x",
            ZhiziPlan::OnDemand6x => "6x",
            ZhiziPlan::OnDemand12x => "12x",
            ZhiziPlan::OnDemand24x => "24x",
        };
        let backend = if self.plan == ZhiziPlan::OnDemand1xCuda {
            "CUDA"
        } else {
            "TENSORRT"
        };
        let weight = match self.weight {
            ZhiziWeight::B20 => "20b",
            ZhiziWeight::Transformer10b384 => "10b384t",
            ZhiziWeight::Transformer10b512 => "10b512t",
            ZhiziWeight::Transformer11b768 => "11b768t",
            ZhiziWeight::B18Nbt => "18bnbt",
            ZhiziWeight::B28Nbt => "28bnbt",
            ZhiziWeight::Fdx => "fdx",
            ZhiziWeight::B60 => "60b",
            ZhiziWeight::B40 => "40b",
        };
        format!(
            "--platform all --engine-type go --gpu-type {gpu} --kata-name katago-{backend} --kata-weight {weight}"
        )
    }

    pub fn profile(self) -> EngineProfile {
        EngineProfile {
            remote: Some(RemoteEngineConfig::Zhizi {
                options: self,
                version: None,
            }),
            ..EngineProfile::default()
        }
    }
}

#[cfg(test)]
mod tests;
