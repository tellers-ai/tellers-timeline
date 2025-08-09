use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct TimelineWasm {
    inner: tellers_timeline_core::Timeline,
}

#[wasm_bindgen]
impl TimelineWasm {
    #[wasm_bindgen(constructor)]
    pub fn new() -> TimelineWasm { TimelineWasm { inner: tellers_timeline_core::Timeline::default() } }

    #[wasm_bindgen(js_name = parseJson)]
    pub fn parse_json(s: &str) -> Result<TimelineWasm, JsValue> {
        let tl: tellers_timeline_core::Timeline = serde_json::from_str(s).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(TimelineWasm { inner: tl })
    }

    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string_pretty(&self.inner).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen]
    pub fn validate(&self) -> js_sys::Array {
        tellers_timeline_core::validate_timeline(&self.inner)
            .into_iter()
            .map(|e| JsValue::from_str(&e.to_string()))
            .collect()
    }
}
