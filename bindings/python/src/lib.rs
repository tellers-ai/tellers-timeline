use pyo3::prelude::*;
// use pyo3::types::PyList;
use pyo3::types::{PyAny, PyDict};
use tellers_timeline_core::to_json_with_precision;
use tellers_timeline_core::track_methods::track_item_insert::{InsertPolicy, OverlapPolicy};
use tellers_timeline_core::{
    validate_timeline, Clip, Effect, EffectMetadata, Gap, InsertItemAtTimeResult, Item, MediaReference, MediaReferencePosition, RationalTime, Stack, TimeRange, Timeline,
    Track, TrackKind,
};
use tellers_timeline_core::{IdMetadataExt, MetadataExt};

#[pyclass(name = "MediaReference")]
#[derive(Clone)]
struct PyMediaReference {
    inner: MediaReference,
}

#[pymethods]
impl PyMediaReference {
    #[new]
    #[pyo3(signature = (url, name=None, media_start=None, media_duration=None, metadata_json=None))]
    fn new(
        url: String,
        name: Option<String>,
        media_start: Option<f64>,
        media_duration: Option<f64>,
        metadata_json: Option<String>,
    ) -> PyResult<Self> {
        let mut metadata = if let Some(s) = metadata_json {
            let v: serde_json::Value = serde_json::from_str(&s)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
            if v.as_object().is_none() {
                serde_json::Value::Object(serde_json::Map::new())
            } else {
                v
            }
        } else {
            serde_json::Value::Object(serde_json::Map::new())
        };

        // Default to empty object
        if metadata.as_object().is_none() {
            metadata = serde_json::Value::Object(serde_json::Map::new());
        }

        let mut inner = MediaReference::ExternalReference {
            target_url: url,
            available_range: None,
            name: Some(name.unwrap_or_default()),
            available_image_bounds: Some(serde_json::Value::Null),
            metadata,
        };

        if let Some(ms) = media_start {
            inner.set_media_start(ms);
        }
        if let Some(md) = media_duration {
            inner.set_media_duration(Some(md));
        }

        Ok(Self { inner })
    }
    fn get_url(&self) -> PyResult<String> {
        self.inner.target_url()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "MediaReference is not an ExternalReference (no target_url)"
            ))
            .map(|s| s.clone())
    }
    fn set_url(&mut self, url: String) -> PyResult<()> {
        match &mut self.inner {
            MediaReference::ExternalReference { target_url, .. } => {
                *target_url = url;
                Ok(())
            }
            MediaReference::GeneratorReference { .. } => {
                Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    "Cannot set target_url on GeneratorReference"
                ))
            }
        }
    }
    fn get_metadata_json(&self) -> PyResult<String> {
        serde_json::to_string(self.inner.metadata())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
    fn set_metadata_json(&mut self, metadata_json: &str) -> PyResult<()> {
        let v: serde_json::Value = serde_json::from_str(metadata_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let coerced = if v.as_object().is_none() {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            v
        };
        *self.inner.metadata_mut() = coerced;
        Ok(())
    }
    fn get_media_start(&self) -> f64 {
        self.inner.media_start()
    }
    fn set_media_start(&mut self, value: f64) {
        self.inner.set_media_start(value);
    }
    fn get_media_duration(&self) -> Option<f64> {
        self.inner.media_duration()
    }
    fn set_media_duration(&mut self, value: Option<f64>) {
        self.inner.set_media_duration(value);
    }
    fn get_rich_text(&self) -> Option<String> {
        self.inner.get_rich_text()
    }
    #[staticmethod]
    fn create_rich_text_reference(title_html: String) -> Self {
        Self {
            inner: MediaReference::create_rich_text_reference(title_html),
        }
    }
    #[staticmethod]
    fn parse_json(s: &str) -> PyResult<Self> {
        let media_ref: MediaReference = serde_json::from_str(s)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { inner: media_ref })
    }
    fn __str__(&self) -> PyResult<String> {
        to_json_with_precision(&self.inner, None, false)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
}

#[pyclass(name = "Effect")]
#[derive(Clone)]
struct PyEffect {
    inner: Effect,
}

#[pymethods]
impl PyEffect {
    #[new]
    #[pyo3(signature = (name=None, effect_name=None, metadata_json=None))]
    fn new(
        name: Option<String>,
        effect_name: Option<String>,
        metadata_json: Option<String>,
    ) -> PyResult<Self> {
        let mut metadata = if let Some(s) = metadata_json {
            let v: serde_json::Value = serde_json::from_str(&s)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
            if v.as_object().is_none() {
                serde_json::Value::Object(serde_json::Map::new())
            } else {
                v
            }
        } else {
            serde_json::Value::Object(serde_json::Map::new())
        };

        // Default to empty object
        if metadata.as_object().is_none() {
            metadata = serde_json::Value::Object(serde_json::Map::new());
        }

        let metadata_typed: EffectMetadata = serde_json::from_value(metadata)
            .unwrap_or_else(|_| EffectMetadata::default());

        let inner = Effect {
            otio_schema: "Effect.1".to_string(),
            name: name.unwrap_or_default(),
            effect_name: effect_name.unwrap_or_default(),
            metadata: metadata_typed,
        };

        Ok(Self { inner })
    }

    fn get_name(&self) -> String {
        self.inner.name.clone()
    }

    fn set_name(&mut self, name: String) {
        self.inner.name = name;
    }

    fn get_effect_name(&self) -> String {
        self.inner.effect_name.clone()
    }

    fn set_effect_name(&mut self, effect_name: String) {
        self.inner.effect_name = effect_name;
    }

    fn get_metadata_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner.metadata)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    fn set_metadata_json(&mut self, metadata_json: &str) -> PyResult<()> {
        let v: serde_json::Value = serde_json::from_str(metadata_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let coerced = if v.as_object().is_none() {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            v
        };
        let metadata_typed: EffectMetadata = serde_json::from_value(coerced)
            .unwrap_or_else(|_| EffectMetadata::default());
        self.inner.metadata = metadata_typed;
        Ok(())
    }

    fn __str__(&self) -> PyResult<String> {
        to_json_with_precision(&self.inner, None, false)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
}

#[pyclass(name = "MediaReferencePosition")]
#[derive(Clone)]
struct PyMediaReferencePosition {
    inner: MediaReferencePosition,
}

#[pymethods]
impl PyMediaReferencePosition {
    #[new]
    #[pyo3(signature = (x=0.0, y=0.0, rotation=0.0, zoom_x=1.0, zoom_y=1.0))]
    fn new(x: f64, y: f64, rotation: f64, zoom_x: f64, zoom_y: f64) -> Self {
        Self {
            inner: MediaReferencePosition {
                x,
                y,
                rotation,
                zoom_x,
                zoom_y,
            },
        }
    }
    fn get_x(&self) -> f64 {
        self.inner.x
    }
    fn set_x(&mut self, x: f64) {
        self.inner.x = x;
    }
    fn get_y(&self) -> f64 {
        self.inner.y
    }
    fn set_y(&mut self, y: f64) {
        self.inner.y = y;
    }
    fn get_rotation(&self) -> f64 {
        self.inner.rotation
    }
    fn set_rotation(&mut self, rotation: f64) {
        self.inner.rotation = rotation;
    }
    fn get_zoom_x(&self) -> f64 {
        self.inner.zoom_x
    }
    fn set_zoom_x(&mut self, zoom_x: f64) {
        self.inner.zoom_x = zoom_x;
    }
    fn get_zoom_y(&self) -> f64 {
        self.inner.zoom_y
    }
    fn set_zoom_y(&mut self, zoom_y: f64) {
        self.inner.zoom_y = zoom_y;
    }
}

#[pyclass(name = "Clip")]
#[derive(Clone)]
struct PyClip {
    inner: Clip,
}

fn dict_to_media_references(
    dict_any: &Bound<PyAny>,
) -> PyResult<std::collections::HashMap<String, MediaReference>> {
    let dict = dict_any.downcast::<PyDict>()?;
    let mut out = std::collections::HashMap::<String, MediaReference>::new();
    for (k, v) in dict {
        let key: String = k.extract().map_err(|_| {
            PyErr::new::<pyo3::exceptions::PyTypeError, _>("media_references keys must be str")
        })?;
        if let Ok(py_ref) = v.extract::<PyRef<PyMediaReference>>() {
            out.insert(key, py_ref.inner.clone());
        } else {
            return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "media_references values must be MediaReference",
            ));
        }
    }
    Ok(out)
}

#[pymethods]
impl PyClip {
    #[new]
    #[pyo3(signature = (duration, references, active_key=None, name=None, id=None))]
    fn new(
        duration: f64,
        references: &Bound<PyAny>,
        active_key: Option<String>,
        name: Option<String>,
        id: Option<String>,
    ) -> PyResult<Self> {
        let rt = RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: duration,
        };
        let sr = TimeRange {
            otio_schema: "TimeRange.1".to_string(),
            duration: rt,
            start_time: RationalTime {
                otio_schema: "RationalTime.1".to_string(),
                rate: 1.0,
                value: 0.0,
            },
        };
        let refs = dict_to_media_references(references)?;
        let mut inner = Clip::new(sr, refs, active_key, name, id);
        if inner.active_media_reference_key.is_none() {
            inner.bind_default_media_reference_when_needed();
        }
        Ok(Self { inner })
    }
    fn get_name(&self) -> Option<String> {
        self.inner.name.clone()
    }
    fn set_name(&mut self, name: Option<String>) {
        self.inner.name = name;
    }
    fn get_enabled(&self) -> bool {
        self.inner.enabled
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.inner.enabled = enabled;
    }
    fn get_duration(&self) -> f64 {
        self.inner.source_range.duration.value
    }
    fn set_duration(&mut self, v: f64) {
        self.inner.source_range.duration.value = v;
    }
    fn get_media_references(&self, py: Python<'_>) -> Py<PyDict> {
        let d = PyDict::new(py);
        for (k, v) in &self.inner.media_references {
            d.set_item(
                k,
                Py::new(py, PyMediaReference { inner: v.clone() }).unwrap(),
            )
            .unwrap();
        }
        d.into_py(py)
    }
    fn set_media_references(&mut self, references: &Bound<PyAny>) -> PyResult<()> {
        self.inner.media_references = dict_to_media_references(references)?;
        Ok(())
    }
    fn get_active_media_reference_key(&self) -> Option<String> {
        self.inner.active_media_reference_key.clone()
    }
    fn set_active_media_reference_key(&mut self, key: Option<String>) -> PyResult<()> {
        if let Some(k) = &key {
            if !self.inner.media_references.contains_key(k) {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    "active key must exist in media_references",
                ));
            }
        }
        self.inner.active_media_reference_key = key;
        Ok(())
    }
    fn get_id(&self) -> Option<String> {
        self.inner.get_id()
    }
    fn set_id(&mut self, id: Option<&str>) {
        self.inner.set_id(id.map(|s| s.to_string()));
    }
    fn get_metadata_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner.metadata)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
    fn set_metadata_json(&mut self, metadata_json: &str) -> PyResult<()> {
        let v: serde_json::Value = serde_json::from_str(metadata_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let coerced = if v.as_object().is_none() {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            v
        };
        self.inner.metadata = coerced;
        Ok(())
    }
    fn __str__(&self) -> PyResult<String> {
        to_json_with_precision(&self.inner, None, false)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
    fn get_effects(&self, py: Python<'_>) -> Vec<Py<PyEffect>> {
        self.inner.effects.iter().map(|e| Py::new(py, PyEffect { inner: e.clone() }).unwrap()).collect()
    }
    fn set_effects(&mut self, effects: Vec<PyEffect>) {
        self.inner.effects = effects.into_iter().map(|e| e.inner).collect();
    }
    fn get_position(&self, py: Python<'_>) -> Py<PyMediaReferencePosition> {
        let pos = self.inner.get_position();
        Py::new(py, PyMediaReferencePosition { inner: pos }).unwrap()
    }
    fn set_position(&mut self, position: PyRef<PyMediaReferencePosition>) {
        self.inner.set_position(position.inner.clone());
    }
    fn get_volume(&self) -> f64 {
        self.inner.get_volume()
    }
    fn set_volume(&mut self, volume: f64) {
        self.inner.set_volume(volume);
    }
    #[staticmethod]
    fn parse_json(s: &str) -> PyResult<Self> {
        let clip: Clip = serde_json::from_str(s)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { inner: clip })
    }
    fn to_json(&self) -> PyResult<String> {
        to_json_with_precision(&self.inner, None, false)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
}

#[pyclass(name = "Gap")]
#[derive(Clone)]
struct PyGap {
    inner: Gap,
}

#[pymethods]
impl PyGap {
    fn get_name(&self) -> Option<String> {
        self.inner.name.clone()
    }
    fn set_name(&mut self, name: Option<String>) {
        self.inner.name = name;
    }
    #[new]
    #[pyo3(signature = (duration, id=None))]
    fn new(duration: f64, id: Option<String>) -> Self {
        let inner = Gap::new(duration, id);
        Self { inner }
    }
    fn get_duration(&self) -> f64 {
        self.inner.source_range.duration.value
    }
    fn set_duration(&mut self, v: f64) {
        self.inner.source_range.duration.value = v;
    }
    fn get_id(&self) -> Option<String> {
        self.inner.get_id()
    }
    fn set_id(&mut self, id: Option<&str>) {
        self.inner.set_id(id.map(|s| s.to_string()));
    }
    fn get_metadata_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner.metadata)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
    fn set_metadata_json(&mut self, metadata_json: &str) -> PyResult<()> {
        let v: serde_json::Value = serde_json::from_str(metadata_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let coerced = if v.as_object().is_none() {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            v
        };
        self.inner.metadata = coerced;
        Ok(())
    }
    fn __str__(&self) -> PyResult<String> {
        to_json_with_precision(&self.inner, None, false)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
    fn get_effects(&self, py: Python<'_>) -> Vec<Py<PyEffect>> {
        self.inner.effects.iter().map(|e| Py::new(py, PyEffect { inner: e.clone() }).unwrap()).collect()
    }
    fn set_effects(&mut self, effects: Vec<PyEffect>) {
        self.inner.effects = effects.into_iter().map(|e| e.inner).collect();
    }
}

#[pyclass(name = "Item")]
#[derive(Clone)]
struct PyItem {
    inner: Item,
}

#[pyclass(name = "TimeRange")]
#[derive(Clone)]
struct PyTimeRange {
    inner: TimeRange,
}

#[pymethods]
impl PyTimeRange {
    #[new]
    #[pyo3(signature = (duration, start_time=0.0))]
    fn new(duration: f64, start_time: f64) -> Self {
        Self {
            inner: TimeRange::new(duration, start_time),
        }
    }
    fn get_duration(&self) -> f64 {
        self.inner.duration.value
    }
    fn set_duration(&mut self, duration: f64) {
        self.inner.duration.value = duration;
    }
    fn get_start_time(&self) -> f64 {
        self.inner.start_time.value
    }
    fn set_start_time(&mut self, start_time: f64) {
        self.inner.start_time.value = start_time;
    }
    fn __str__(&self) -> PyResult<String> {
        to_json_with_precision(&self.inner, None, false)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
}

#[pymethods]
impl PyItem {
    #[staticmethod]
    fn from_clip(c: PyClip) -> Self {
        Self {
            inner: Item::Clip(c.inner),
        }
    }
    #[staticmethod]
    fn from_gap(g: PyGap) -> Self {
        Self {
            inner: Item::Gap(g.inner),
        }
    }
    fn is_clip(&self) -> bool {
        matches!(self.inner, Item::Clip(_))
    }
    fn is_gap(&self) -> bool {
        matches!(self.inner, Item::Gap(_))
    }
    fn duration(&self) -> f64 {
        self.inner.duration()
    }
    fn set_duration(&mut self, dur: f64) {
        self.inner.set_duration(dur);
    }
    fn get_enabled(&self) -> bool {
        self.inner.get_enabled()
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.inner.set_enabled(enabled);
    }
    fn get_id(&self) -> Option<String> {
        self.inner.get_id()
    }
    fn set_id(&mut self, id: Option<&str>) {
        self.inner.set_id(id.map(|s| s.to_string()));
    }
    fn get_metadata_json(&self) -> PyResult<String> {
        serde_json::to_string(self.inner.get_metadata())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
    fn set_metadata_json(&mut self, metadata_json: &str) -> PyResult<()> {
        let v: serde_json::Value = serde_json::from_str(metadata_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        self.inner.set_metadata(v);
        Ok(())
    }
    fn get_active_media_reference_key(&self) -> Option<String> {
        self.inner.get_active_media_reference_key()
    }
    fn set_active_media_reference_key(&mut self, key: Option<&str>) {
        self.inner
            .set_active_media_reference_key(key.map(|s| s.to_string()));
    }
    fn get_media_references(&self, py: Python<'_>) -> Py<PyDict> {
        let d = PyDict::new(py);
        for (k, v) in self.inner.get_media_references() {
            d.set_item(k, Py::new(py, PyMediaReference { inner: v }).unwrap())
                .unwrap();
        }
        d.into_py(py)
    }
    fn set_media_references(&mut self, references: &Bound<PyAny>) -> PyResult<()> {
        let refs = dict_to_media_references(references)?;
        self.inner.set_media_references(refs);
        Ok(())
    }
    fn get_source_range(&self, py: Python<'_>) -> Py<PyTimeRange> {
        let tr = self.inner.get_source_range();
        Py::new(py, PyTimeRange { inner: tr }).unwrap()
    }
    fn set_source_range(&mut self, tr: PyTimeRange) {
        self.inner.set_source_range(tr.inner);
    }
    fn __str__(&self) -> PyResult<String> {
        to_json_with_precision(&self.inner, None, false)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
    fn get_effects(&self, py: Python<'_>) -> Vec<Py<PyEffect>> {
        self.inner.get_effects().iter().map(|e| Py::new(py, PyEffect { inner: e.clone() }).unwrap()).collect()
    }
    fn set_effects(&mut self, effects: Vec<PyEffect>) {
        self.inner.set_effects(effects.into_iter().map(|e| e.inner).collect());
    }
    fn get_position(&self, py: Python<'_>) -> Py<PyMediaReferencePosition> {
        let pos = self.inner.get_position();
        Py::new(py, PyMediaReferencePosition { inner: pos }).unwrap()
    }
    fn set_position(&mut self, position: PyRef<PyMediaReferencePosition>) {
        self.inner.set_position(position.inner.clone());
    }
    fn get_volume(&self) -> f64 {
        self.inner.get_volume()
    }
    fn set_volume(&mut self, volume: f64) {
        self.inner.set_volume(volume);
    }
}

#[pyclass(name = "Track")]
#[derive(Clone)]
struct PyTrack {
    inner: Track,
}

fn track_kind_from_str(s: &str) -> TrackKind {
    match s.to_ascii_lowercase().as_str() {
        "video" => TrackKind::Video,
        "audio" => TrackKind::Audio,
        _ => TrackKind::Other,
    }
}

fn clamp_insertion_index(len: usize, index: isize) -> usize {
    if index < 0 {
        let pos = len as isize + index;
        if pos <= 0 {
            0
        } else if pos >= len as isize {
            len
        } else {
            pos as usize
        }
    } else {
        let idx = index as usize;
        if idx > len {
            len
        } else {
            idx
        }
    }
}

fn overlap_policy_from_str(s: &str) -> OverlapPolicy {
    match s.to_ascii_lowercase().as_str() {
        "override" => OverlapPolicy::Override,
        "push" => OverlapPolicy::Push,
        _ => OverlapPolicy::Override,
    }
}

fn insert_policy_from_str(s: &str) -> InsertPolicy {
    match s.to_ascii_lowercase().as_str() {
        "split_and_insert" | "split" => InsertPolicy::SplitAndInsert,
        "insert_before" | "before" => InsertPolicy::InsertBefore,
        "insert_after" | "after" => InsertPolicy::InsertAfter,
        _ => InsertPolicy::InsertBeforeOrAfter,
    }
}

#[pymethods]
impl PyTrack {
    fn get_name(&self) -> Option<String> {
        self.inner.name.clone()
    }
    fn set_name(&mut self, name: Option<String>) {
        self.inner.name = name;
    }
    fn get_enabled(&self) -> bool {
        self.inner.enabled
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.inner.enabled = enabled;
    }
    #[new]
    #[pyo3(signature = (kind=None, id=None, children=None))]
    fn new(
        kind: Option<String>,
        id: Option<String>,
        children: Option<Vec<PyItem>>,
    ) -> Self {
        let k = kind
            .map(|s| track_kind_from_str(&s))
            .unwrap_or(TrackKind::Video);
        let mut inner = Track::new(k, id);
        if let Some(items) = children {
            inner.items = items.into_iter().map(|i| i.inner).collect();
        }
        Self { inner }
    }
    #[getter]
    fn kind(&self) -> String {
        match &self.inner.kind {
            TrackKind::Video => "video".to_string(),
            TrackKind::Audio => "audio".to_string(),
            TrackKind::Other => "other".to_string(),
        }
    }
    #[setter]
    fn set_kind(&mut self, kind: String) {
        self.inner.kind = track_kind_from_str(&kind);
    }
    fn items(&self, py: Python<'_>) -> Vec<Py<PyItem>> {
        self.inner
            .items
            .iter()
            .cloned()
            .map(|it| Py::new(py, PyItem { inner: it }).unwrap())
            .collect()
    }
    fn get_items(&self, py: Python<'_>) -> Vec<Py<PyItem>> {
        self.inner
            .items
            .iter()
            .cloned()
            .map(|it| Py::new(py, PyItem { inner: it }).unwrap())
            .collect()
    }
    fn timeline_ids(&self) -> Vec<String> {
        self.inner.timeline_ids()
    }
    fn get_id(&self) -> Option<String> {
        self.inner.get_id()
    }
    fn get_item_at_time(&self, time: f64) -> Option<usize> {
        self.inner.get_item_at_time(time)
    }
    fn get_item_by_id(&self, py: Python<'_>, id: &str) -> Option<(usize, Py<PyItem>)> {
        self.inner.get_item_by_id(id).map(|(i, _it)| {
            let item = self.inner.items[i].clone();
            (i, Py::new(py, PyItem { inner: item }).unwrap())
        })
    }
    fn total_duration(&self) -> f64 {
        self.inner.total_duration()
    }
    fn start_time_of_item(&self, index: usize) -> f64 {
        self.inner.start_time_of_item(index)
    }
    fn get_metadata_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner.metadata)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
    fn __str__(&self) -> PyResult<String> {
        to_json_with_precision(&self.inner, None, false)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
}

#[pyclass(name = "Stack")]
#[derive(Clone)]
struct PyStack {
    inner: Stack,
}

#[pymethods]
impl PyStack {
    #[new]
    #[pyo3(signature = (children=None))]
    fn new(children: Option<Vec<PyTrack>>) -> Self {
        let mut inner = Stack::default();
        if let Some(tracks) = children {
            inner.children = tracks.into_iter().map(|t| t.inner).collect();
        }
        Self { inner }
    }
    fn get_name(&self) -> Option<String> {
        self.inner.name.clone()
    }
    fn set_name(&mut self, name: Option<String>) {
        self.inner.name = name;
    }
    fn tracks(&self, py: Python<'_>) -> Vec<Py<PyTrack>> {
        self.inner
            .children
            .iter()
            .cloned()
            .map(|t| Py::new(py, PyTrack { inner: t }).unwrap())
            .collect()
    }
    fn children(&self, py: Python<'_>) -> Vec<Py<PyTrack>> {
        self.inner
            .children
            .iter()
            .cloned()
            .map(|t| Py::new(py, PyTrack { inner: t }).unwrap())
            .collect()
    }
    fn clear_children(&mut self) {
        self.inner.children.clear();
    }
    #[pyo3(signature = (track, insertion_index=-1))]
    fn add_track(&mut self, track: PyTrack, insertion_index: isize) -> PyResult<String> {
        let insert_index = clamp_insertion_index(self.inner.children.len(), insertion_index);
        if !self.inner.add_track_at(track.inner, insertion_index) {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Failed to add track",
            ));
        }
        self.inner.children[insert_index].get_id().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>("Inserted track has no id")
        })
    }
    fn reorder_track(&mut self, id: &str, insertion_index: isize) -> bool {
        self.inner.reorder_track(id, insertion_index)
    }
    fn sync_track_info(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        sync_track_info_to_python(py, self.inner.sync_track_info())
    }
    fn delete_track(&mut self, py: Python<'_>, id: &str) -> Option<Py<PyTrack>> {
        self.inner
            .delete_track(id)
            .map(|t| Py::new(py, PyTrack { inner: t }).unwrap())
    }
    fn set_tracks(&mut self, tracks: Vec<PyTrack>) {
        self.inner.children = tracks.into_iter().map(|t| t.inner).collect();
    }
    fn sanitize(&mut self) {
        self.inner.sanitize();
    }
    fn get_metadata_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner.metadata)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
    fn set_metadata_json(&mut self, metadata_json: &str) -> PyResult<()> {
        let v: serde_json::Value = serde_json::from_str(metadata_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let coerced = if v.as_object().is_none() {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            v
        };
        self.inner.metadata = coerced;
        Ok(())
    }
    fn __str__(&self) -> PyResult<String> {
        to_json_with_precision(&self.inner, None, false)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
    fn get_track_by_id(&self, py: Python<'_>, id: &str) -> Option<(usize, Py<PyTrack>)> {
        self.inner.get_track_by_id(id).map(|(i, _t)| {
            let tr = self.inner.children[i].clone();
            (i, Py::new(py, PyTrack { inner: tr }).unwrap())
        })
    }
    fn get_item(&self, py: Python<'_>, id: &str) -> Option<(usize, usize, Py<PyItem>)> {
        self.inner.get_item(id).map(|(ti, ii, _it)| {
            let item = self.inner.children[ti].items[ii].clone();
            (ti, ii, Py::new(py, PyItem { inner: item }).unwrap())
        })
    }
    #[pyo3(signature = (id, replace_with_gap))]
    fn delete_item(
        &mut self,
        py: Python<'_>,
        id: &str,
        replace_with_gap: bool,
    ) -> Vec<(usize, Py<PyItem>)> {
        self.inner
            .delete_item(id, replace_with_gap)
            .into_iter()
            .map(|(ti, it)| (ti, Py::new(py, PyItem { inner: it }).unwrap()))
            .collect()
    }
    #[pyo3(signature = (dest_track_index, dest_time, item, overlap_policy, insert_policy, linked_audio_clips=None, linked_video_clip=None))]
    fn insert_item_at_time(
        &mut self,
        py: Python<'_>,
        dest_track_index: usize,
        dest_time: f64,
        item: &Bound<PyAny>,
        overlap_policy: &str,
        insert_policy: &str,
        linked_audio_clips: Option<Vec<PyObject>>,
        linked_video_clip: Option<PyObject>,
    ) -> PyResult<Option<PyObject>> {
        if let Some(inner_item) = extract_item(item) {
            let linked_video_clip = extract_optional_linked_clip(
                py,
                linked_video_clip,
                "linked_video_clip",
            )?;
            if (linked_audio_clips.is_some() || linked_video_clip.is_some())
                && !matches!(inner_item, Item::Clip(_))
            {
                return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                    "linked_audio_clips and linked_video_clip can only be used when item is a Clip",
                ));
            }
            let op = overlap_policy_from_str(overlap_policy);
            let ip = insert_policy_from_str(insert_policy);
            let linked_audio_clips = linked_audio_clips
                .map(|items| {
                    items
                        .into_iter()
                        .map(|item| {
                            extract_item(item.bind(py)).ok_or_else(|| {
                                PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                                    "linked_audio_clips expects Item or Clip values",
                                )
                            })
                        })
                        .collect::<PyResult<Vec<_>>>()
                })
                .transpose()?;
            match self.inner.insert_item_at_time(
                dest_track_index,
                dest_time,
                inner_item,
                op,
                ip,
                linked_audio_clips,
                linked_video_clip,
            ) {
                Some(InsertItemAtTimeResult::ItemId(id)) => Ok(Some(id.into_py(py))),
                Some(InsertItemAtTimeResult::Synced(result)) => {
                    let dict = PyDict::new(py);
                    dict.set_item("primary_clip_id", result.primary_clip_id)?;
                    dict.set_item("audio_clips", result.audio_clips)?;
                    dict.set_item("linked_video_clip_id", result.synced_video_clip_id)?;
                    dict.set_item("link_group_id", result.sync_clips_id)?;
                    dict.set_item("created_track_indices", result.created_track_indices)?;
                    Ok(Some(dict.into_py(py)))
                }
                None => Ok(None),
            }
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "insert_item_at_time expects an Item, Clip, or Gap",
            ))
        }
    }
    #[pyo3(signature = (dest_track_id, dest_index, item, overlap_policy, linked_audio_clips=None, linked_video_clip=None))]
    fn insert_item_at_index(
        &mut self,
        py: Python<'_>,
        dest_track_id: &str,
        dest_index: usize,
        item: &Bound<PyAny>,
        overlap_policy: &str,
        linked_audio_clips: Option<Vec<PyObject>>,
        linked_video_clip: Option<PyObject>,
    ) -> PyResult<Option<PyObject>> {
        if let Some(inner_item) = extract_item(item) {
            let linked_video_clip = extract_optional_linked_clip(
                py,
                linked_video_clip,
                "linked_video_clip",
            )?;
            if (linked_audio_clips.is_some() || linked_video_clip.is_some())
                && !matches!(inner_item, Item::Clip(_))
            {
                return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                    "linked_audio_clips and linked_video_clip can only be used when item is a Clip",
                ));
            }
            let op = overlap_policy_from_str(overlap_policy);
            let linked_audio_clips = linked_audio_clips
                .map(|items| {
                    items
                        .into_iter()
                        .map(|item| {
                            extract_item(item.bind(py)).ok_or_else(|| {
                                PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                                    "linked_audio_clips expects Item or Clip values",
                                )
                            })
                        })
                        .collect::<PyResult<Vec<_>>>()
                })
                .transpose()?;
            match self
                .inner
                .insert_item_at_index(
                    dest_track_id,
                    dest_index,
                    inner_item,
                    op,
                    linked_audio_clips,
                    linked_video_clip,
                )
            {
                Some(InsertItemAtTimeResult::ItemId(id)) => Ok(Some(id.into_py(py))),
                Some(InsertItemAtTimeResult::Synced(result)) => {
                    let dict = PyDict::new(py);
                    dict.set_item("primary_clip_id", result.primary_clip_id)?;
                    dict.set_item("audio_clips", result.audio_clips)?;
                    dict.set_item("linked_video_clip_id", result.synced_video_clip_id)?;
                    dict.set_item("link_group_id", result.sync_clips_id)?;
                    dict.set_item("created_track_indices", result.created_track_indices)?;
                    Ok(Some(dict.into_py(py)))
                }
                None => Ok(None),
            }
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "insert_item_at_index expects an Item, Clip, or Gap",
            ))
        }
    }
    fn unlink_item(&mut self, item_ids: Vec<String>) -> usize {
        self.inner.unsync_item(&item_ids)
    }
    fn link_item(&mut self, item_ids: Vec<String>) -> Option<i64> {
        self.inner.sync_item(&item_ids)
    }
    fn group_item(&mut self, item_ids: Vec<String>) -> Option<i64> {
        self.inner.group_item(&item_ids)
    }
    fn ungroup_item(&mut self, item_ids: Vec<String>) -> usize {
        self.inner.ungroup_item(&item_ids)
    }
    fn move_item_at_time(
        &mut self,
        item_id: &str,
        dest_track_id: &str,
        dest_time: f64,
        replace_with_gap: bool,
        overlap_policy: &str,
        insert_policy: &str,
    ) -> PyResult<bool> {
        let op = overlap_policy_from_str(overlap_policy);
        let ip = insert_policy_from_str(insert_policy);
        Ok(self.inner.move_item_at_time(
            item_id,
            dest_track_id,
            dest_time,
            replace_with_gap,
            ip,
            op,
        ))
    }
    fn move_item_at_index(
        &mut self,
        item_id: &str,
        dest_track_id: &str,
        dest_index: usize,
        replace_with_gap: bool,
        overlap_policy: &str,
    ) -> PyResult<bool> {
        let op = overlap_policy_from_str(overlap_policy);
        Ok(self
            .inner
            .move_item_at_index(item_id, dest_track_id, dest_index, replace_with_gap, op))
    }
    fn split_item_at_time(&mut self, item_id: &str, split_time: f64) -> bool {
        self.inner.split_item_at_time(item_id, split_time)
    }
    fn resize_item(
        &mut self,
        item_id: &str,
        new_start_time: f64,
        new_duration: f64,
        overlap_policy: &str,
        clamp_to_media: bool,
    ) -> bool {
        let op = overlap_policy_from_str(overlap_policy);
        self.inner
            .resize_item(item_id, new_start_time, new_duration, op, clamp_to_media)
    }
    #[pyo3(signature = (item_id, item, linked_audio_clips=None))]
    fn replace_item(
        &mut self,
        py: Python<'_>,
        item_id: &str,
        item: &Bound<PyAny>,
        linked_audio_clips: Option<Vec<PyObject>>,
    ) -> PyResult<bool> {
        if let Some(inner_item) = extract_item(item) {
            if linked_audio_clips.is_some() && !matches!(inner_item, Item::Clip(_)) {
                return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                    "linked_audio_clips can only be used when item is a Clip",
                ));
            }
            let linked_audio_clips = linked_audio_clips
                .map(|items| {
                    items
                        .into_iter()
                        .map(|item| {
                            extract_item(item.bind(py)).ok_or_else(|| {
                                PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                                    "linked_audio_clips expects Item or Clip values",
                                )
                            })
                        })
                        .collect::<PyResult<Vec<_>>>()
                })
                .transpose()?;
            Ok(self
                .inner
                .replace_item(item_id, inner_item, linked_audio_clips))
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "replace_item expects an Item, Clip, or Gap",
            ))
        }
    }
}

fn extract_item(item: &Bound<PyAny>) -> Option<Item> {
    if let Ok(py_item) = item.extract::<PyRef<PyItem>>() {
        return Some(py_item.inner.clone());
    }
    if let Ok(py_clip) = item.extract::<PyRef<PyClip>>() {
        return Some(Item::Clip(py_clip.inner.clone()));
    }
    if let Ok(py_gap) = item.extract::<PyRef<PyGap>>() {
        return Some(Item::Gap(py_gap.inner.clone()));
    }
    None
}

fn extract_optional_linked_clip(
    py: Python<'_>,
    item: Option<PyObject>,
    param_name: &str,
) -> PyResult<Option<Item>> {
    item.map(|item| {
        let item = extract_item(item.bind(py)).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyTypeError, _>(format!(
                "{param_name} expects Item or Clip values"
            ))
        })?;
        if matches!(item, Item::Gap(_)) {
            return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(format!(
                "{param_name} cannot be a Gap"
            )));
        }
        Ok(item)
    })
    .transpose()
}

fn sync_track_info_to_python(
    py: Python<'_>,
    groups: Vec<tellers_timeline_core::SyncTrackInfo>,
) -> PyResult<Vec<PyObject>> {
    groups
        .into_iter()
        .map(|group| {
            let dict = PyDict::new(py);
            dict.set_item("track_indices", group.track_indices)?;
            dict.set_item("track_ids", group.track_ids)?;
            Ok(dict.into_py(py))
        })
        .collect()
}

#[pyclass(name = "Timeline")]
#[derive(Clone)]
struct PyTimeline {
    inner: Timeline,
}

#[pymethods]
impl PyTimeline {
    #[new]
    #[pyo3(signature = (tracks=None))]
    fn new(tracks: Option<&Bound<PyAny>>) -> PyResult<Self> {
        let mut inner = Timeline::default();
        if let Some(arg) = tracks {
            if let Ok(py_stack) = arg.extract::<PyRef<PyStack>>() {
                inner.tracks = py_stack.inner.clone();
            } else if let Ok(v_tracks) = arg.extract::<Vec<PyTrack>>() {
                inner.tracks.children = v_tracks.into_iter().map(|t| t.inner).collect();
            } else {
                return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                    "tracks must be a Stack or a list[Track]",
                ));
            }
        }
        Ok(Self { inner })
    }

    #[staticmethod]
    fn parse_json(s: &str) -> PyResult<Self> {
        let tl: Timeline = serde_json::from_str(s)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { inner: tl })
    }

    #[pyo3(signature = (precision=None, pretty=true))]
    fn to_json(&self, precision: Option<usize>, pretty: bool) -> PyResult<String> {
        self.inner
            .to_json_with_options(precision, pretty)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    fn validate(&self) -> Vec<String> {
        validate_timeline(&self.inner)
            .into_iter()
            .map(|e| e.to_string())
            .collect()
    }

    fn sanitize(&mut self) {
        self.inner.sanitize();
    }

    fn get_name(&self) -> Option<String> {
        self.inner.name.clone()
    }
    fn set_name(&mut self, name: Option<String>) {
        self.inner.name = name;
    }
    fn get_stack(&self, py: Python<'_>) -> Py<PyStack> {
        Py::new(
            py,
            PyStack {
                inner: self.inner.tracks.clone(),
            },
        )
        .unwrap()
    }
    fn set_stack(&mut self, stack: PyStack) {
        self.inner.tracks = stack.inner;
    }
    #[pyo3(signature = (track, insertion_index=-1))]
    fn add_track(&mut self, track: PyTrack, insertion_index: isize) -> PyResult<String> {
        let insert_index = clamp_insertion_index(self.inner.tracks.children.len(), insertion_index);
        if !self.inner.add_track_at(track.inner, insertion_index) {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Failed to add track",
            ));
        }
        self.inner.tracks.children[insert_index]
            .get_id()
            .ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>("Inserted track has no id")
            })
    }
    fn reorder_track(&mut self, id: &str, insertion_index: isize) -> bool {
        self.inner.reorder_track(id, insertion_index)
    }
    fn sync_track_info(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        sync_track_info_to_python(py, self.inner.sync_track_info())
    }
    fn delete_track(&mut self, py: Python<'_>, id: &str) -> Option<Py<PyTrack>> {
        self.inner
            .delete_track(id)
            .map(|t| Py::new(py, PyTrack { inner: t }).unwrap())
    }
    fn move_item(&mut self, item_id: &str, dest_track_id: &str, dest_time: f64) -> PyResult<bool> {
        Ok(self.inner.tracks.move_item_at_time(
            item_id,
            dest_track_id,
            dest_time,
            false,
            InsertPolicy::InsertBeforeOrAfter,
            OverlapPolicy::Override,
        ))
    }
    fn get_metadata_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner.metadata)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
    fn set_metadata_json(&mut self, metadata_json: &str) -> PyResult<()> {
        let v: serde_json::Value = serde_json::from_str(metadata_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let coerced = if v.as_object().is_none() {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            v
        };
        self.inner.metadata = coerced;
        Ok(())
    }
    fn __str__(&self) -> PyResult<String> {
        // Use same precision logic as other types, but pretty-print for timelines by default
        self.inner
            .to_json_with_options(None, true)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
}

#[pymodule]
fn tellers_timeline(_py: Python, m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<PyMediaReference>()?;
    m.add_class::<PyMediaReferencePosition>()?;
    m.add_class::<PyEffect>()?;
    m.add_class::<PyClip>()?;
    m.add_class::<PyGap>()?;
    m.add_class::<PyItem>()?;
    m.add_class::<PyTrack>()?;
    m.add_class::<PyStack>()?;
    m.add_class::<PyTimeline>()?;
    Ok(())
}
