use pyo3::prelude::*;
// use pyo3::types::PyList;
use tellers_timeline_core::insert::{InsertPolicy, OverlapPolicy};
use tellers_timeline_core::{validate_timeline, Clip, Gap, Item, MediaSource, Timeline, Track, TrackKind};

#[pyclass(name = "MediaSource")]
#[derive(Clone)]
struct PyMediaSource { inner: MediaSource }

#[pymethods]
impl PyMediaSource {
    #[new]
    fn new(url: String) -> Self {
        Self { inner: MediaSource { otio_schema: "ExternalReference.1".to_string(), url, media_start: 0.0, media_duration: None, metadata: serde_json::Value::Null } }
    }
    fn get_url(&self) -> String { self.inner.url.clone() }
    fn set_url(&mut self, url: String) { self.inner.url = url; }
    fn get_media_start(&self) -> f64 { self.inner.media_start }
    fn set_media_start(&mut self, v: f64) { self.inner.media_start = v; }
    fn get_media_duration(&self) -> Option<f64> { self.inner.media_duration }
    fn set_media_duration(&mut self, v: Option<f64>) { self.inner.media_duration = v; }
}

#[pyclass(name = "Clip")]
#[derive(Clone)]
struct PyClip { inner: Clip }

#[pymethods]
impl PyClip {
    #[new]
    #[pyo3(signature = (duration, source, name=None))]
    fn new(duration: f64, source: PyMediaSource, name: Option<String>) -> Self {
        Self { inner: Clip { otio_schema: "Clip.2".to_string(), name, duration, source: source.inner, metadata: serde_json::Value::Null } }
    }
    fn get_name(&self) -> Option<String> { self.inner.name.clone() }
    fn set_name(&mut self, name: Option<String>) { self.inner.name = name; }
    fn get_duration(&self) -> f64 { self.inner.duration }
    fn set_duration(&mut self, v: f64) { self.inner.duration = v; }
    fn get_source(&self) -> PyMediaSource { PyMediaSource { inner: self.inner.source.clone() } }
    fn set_source(&mut self, source: PyMediaSource) { self.inner.source = source.inner; }
}

#[pyclass(name = "Gap")]
#[derive(Clone)]
struct PyGap { inner: Gap }

#[pymethods]
impl PyGap {
    #[new]
    fn new(duration: f64) -> Self {
        Self { inner: Gap { otio_schema: "Gap.1".to_string(), duration, metadata: serde_json::Value::Null } }
    }
    fn get_duration(&self) -> f64 { self.inner.duration }
    fn set_duration(&mut self, v: f64) { self.inner.duration = v; }
}

#[pyclass(name = "Item")]
#[derive(Clone)]
struct PyItem { inner: Item }

#[pymethods]
impl PyItem {
    #[staticmethod]
    fn from_clip(c: PyClip) -> Self { Self { inner: Item::Clip(c.inner) } }
    #[staticmethod]
    fn from_gap(g: PyGap) -> Self { Self { inner: Item::Gap(g.inner) } }
    fn is_clip(&self) -> bool { matches!(self.inner, Item::Clip(_)) }
    fn is_gap(&self) -> bool { matches!(self.inner, Item::Gap(_)) }
    fn duration(&self) -> f64 { self.inner.duration() }
    fn set_duration(&mut self, dur: f64) { self.inner.set_duration(dur); }
}

#[pyclass(name = "Track")]
#[derive(Clone)]
struct PyTrack { inner: Track }

fn track_kind_from_str(s: &str) -> TrackKind {
    match s.to_ascii_lowercase().as_str() {
        "video" => TrackKind::Video,
        "audio" => TrackKind::Audio,
        other => TrackKind::Other(other.to_string()),
    }
}

fn overlap_policy_from_str(s: &str) -> OverlapPolicy {
    match s.to_ascii_lowercase().as_str() {
        "override" => OverlapPolicy::Override,
        "keep" => OverlapPolicy::Keep,
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
    #[new]
    fn new(kind: Option<String>) -> Self {
        let k = kind.map(|s| track_kind_from_str(&s)).unwrap_or(TrackKind::Video);
        Self { inner: Track { otio_schema: "Track.1".to_string(), kind: k, items: vec![], metadata: serde_json::Value::Null } }
    }
    #[getter]
    fn kind(&self) -> String {
        match &self.inner.kind {
            TrackKind::Video => "video".to_string(),
            TrackKind::Audio => "audio".to_string(),
            TrackKind::Other(s) => s.clone(),
        }
    }
    #[setter]
    fn set_kind(&mut self, kind: String) { self.inner.kind = track_kind_from_str(&kind); }
    fn items(&self, py: Python<'_>) -> Vec<Py<PyItem>> {
        self
            .inner
            .items
            .iter()
            .cloned()
            .map(|it| Py::new(py, PyItem { inner: it }).unwrap())
            .collect()
    }
    fn clear_items(&mut self) { self.inner.items.clear(); }
    fn append(&mut self, item: PyItem) { self.inner.append(item.inner); }
    fn insert_at_index(&mut self, index: usize, item: PyItem) { self.inner.insert_at_index(index, item.inner); }
    fn insert_at_time_with(&mut self, start_time: f64, item: PyItem, overlap_policy: &str, insert_policy: &str) {
        let op = overlap_policy_from_str(overlap_policy);
        let ip = insert_policy_from_str(insert_policy);
        self.inner.insert_at_time_with(start_time, item.inner, op, ip);
    }
    fn total_duration(&self) -> f64 { self.inner.total_duration() }
    fn start_time_of_item(&self, index: usize) -> f64 { self.inner.start_time_of_item(index) }
    #[pyo3(signature = (start_time, duration, url, overlap_policy, insert_policy, name=None, media_start=None, media_duration=None))]
    fn insert_clip(
        &mut self,
        start_time: f64,
        duration: f64,
        url: String,
        overlap_policy: &str,
        insert_policy: &str,
        name: Option<String>,
        media_start: Option<f64>,
        media_duration: Option<f64>,
    ) {
        let ms = MediaSource {
            otio_schema: "ExternalReference.1".to_string(),
            url,
            media_start: media_start.unwrap_or(0.0),
            media_duration,
            metadata: serde_json::Value::Null,
        };
        let clip = Clip { otio_schema: "Clip.2".to_string(), name, duration, source: ms, metadata: serde_json::Value::Null };
        let item = Item::Clip(clip);
        let op = overlap_policy_from_str(overlap_policy);
        let ip = insert_policy_from_str(insert_policy);
        self.inner.insert_at_time_with(start_time, item, op, ip);
    }
}

#[pyclass(name = "Timeline")]
#[derive(Clone)]
struct PyTimeline { inner: Timeline }

#[pymethods]
impl PyTimeline {
    #[new]
    fn new() -> Self { Self { inner: Timeline::default() } }

    #[staticmethod]
    fn parse_json(s: &str) -> PyResult<Self> {
        let tl: Timeline = serde_json::from_str(s).map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { inner: tl })
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(&self.inner).map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    fn validate(&self) -> Vec<String> {
        validate_timeline(&self.inner).into_iter().map(|e| e.to_string()).collect()
    }

    fn sanitize(&mut self) { self.inner.sanitize(); }

    fn get_name(&self) -> Option<String> { self.inner.name.clone() }
    fn set_name(&mut self, name: Option<String>) { self.inner.name = name; }
    fn tracks(&self, py: Python<'_>) -> Vec<Py<PyTrack>> {
        self
            .inner
            .tracks
            .iter()
            .cloned()
            .map(|t| Py::new(py, PyTrack { inner: t }).unwrap())
            .collect()
    }
    fn add_track(&mut self, track: PyTrack) { self.inner.tracks.push(track.inner); }
}

#[pymodule]
fn tellers_timeline(_py: Python, m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<PyMediaSource>()?;
    m.add_class::<PyClip>()?;
    m.add_class::<PyGap>()?;
    m.add_class::<PyItem>()?;
    m.add_class::<PyTrack>()?;
    m.add_class::<PyTimeline>()?;
    Ok(())
}
