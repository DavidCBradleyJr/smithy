//! Health probes. For llama-server we read `/props`, not `/health`: `/health`
//! answers before the weights finish loading, and only `/props` reveals which
//! quant kernels actually engaged.

use super::profiles::{Kind, Profile};
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Stopped,
    Starting,
    Running,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Probe {
    pub state: State,
    /// Models served (llama-server: the loaded file; others: their model list).
    pub models: Vec<String>,
    pub n_ctx: Option<u64>,
    pub ftype: Option<String>,
    /// `None` when the profile has no expectation to check.
    pub ftype_ok: Option<bool>,
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

pub fn from_props(props: &Value, expect_ftype: Option<&str>) -> Probe {
    let ftype = props["model_ftype"].as_str().map(str::to_string);
    let ftype_ok = expect_ftype.map(|want| ftype.as_deref().is_some_and(|f| f.contains(want)));
    Probe {
        state: State::Running,
        models: props["model_path"].as_str().map(basename).into_iter().collect(),
        n_ctx: props["default_generation_settings"]["n_ctx"].as_u64(),
        ftype,
        ftype_ok,
    }
}

/// Model ids from an OpenAI-style `/v1/models` or Ollama `/api/ps` body.
pub fn model_ids(body: &Value) -> Vec<String> {
    let list = body["data"].as_array().or_else(|| body["models"].as_array());
    list.into_iter()
        .flatten()
        .filter_map(|m| m["id"].as_str().or_else(|| m["name"].as_str()))
        .map(str::to_string)
        .collect()
}

pub async fn probe(p: &Profile) -> Probe {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("static client config");
    let base = p.base_url();
    if p.kind == Kind::LlamaRouter {
        return probe_router().await;
    }
    let path = match p.kind {
        Kind::LlamaServer | Kind::LlamaRouter => "/props",
        Kind::LmStudio => "/v1/models",
        Kind::Ollama => "/api/ps",
    };
    let Ok(resp) = client.get(format!("{base}{path}")).send().await else {
        return Probe::default();
    };
    // llama-server answers 503 while loading weights.
    if resp.status().as_u16() == 503 {
        return Probe { state: State::Starting, ..Probe::default() };
    }
    let Ok(body) = resp.json::<Value>().await else {
        return Probe { state: State::Starting, ..Probe::default() };
    };
    match p.kind {
        Kind::LlamaServer => from_props(&body, p.expect_ftype.as_deref()),
        _ => Probe { state: State::Running, models: model_ids(&body), ..Probe::default() },
    }
}

async fn probe_router() -> Probe {
    let st = crate::local::state().await;
    if !st.router {
        return Probe::default();
    }
    let loading = st.models.iter().any(|m| m.status == "loading");
    Probe {
        state: if loading { State::Starting } else { State::Running },
        models: st.models.iter().filter(|m| m.status != "unloaded").map(|m| m.id.clone()).collect(),
        n_ctx: st.loaded.as_ref().and_then(|l| l.n_ctx),
        ftype: st.loaded.and_then(|l| l.ftype),
        ftype_ok: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn props_with_expected_ftype() {
        let props = json!({
            "model_path": "/home/x/.local/share/bonsai/models/Ternary-Bonsai-2-27B-PTQ1_0.gguf",
            "model_ftype": "PTQ1_0 - 1.75 bpw ternary (group 128)",
            "default_generation_settings": { "n_ctx": 262144 }
        });
        let p = from_props(&props, Some("PTQ1_0"));
        assert_eq!(p.n_ctx, Some(262144));
        assert_eq!(p.ftype_ok, Some(true));
        assert_eq!(p.models, vec!["Ternary-Bonsai-2-27B-PTQ1_0.gguf"]);
    }

    #[test]
    fn props_wrong_ftype_is_flagged() {
        let props = json!({ "model_ftype": "Q2_0" });
        assert_eq!(from_props(&props, Some("PTQ1_0")).ftype_ok, Some(false));
        assert_eq!(from_props(&props, None).ftype_ok, None);
    }

    #[test]
    fn model_lists() {
        assert_eq!(model_ids(&json!({"data": [{"id": "a"}, {"id": "b"}]})), vec!["a", "b"]);
        assert_eq!(model_ids(&json!({"models": [{"name": "qwen3:8b"}]})), vec!["qwen3:8b"]);
        assert!(model_ids(&json!({})).is_empty());
    }
}
