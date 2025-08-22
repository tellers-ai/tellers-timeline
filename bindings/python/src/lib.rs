use pyo3::prelude::*;
// use pyo3::types::PyList;
use pyo3::types::PyAny;
use tellers_timeline_core::track_methods::track_item_insert::{InsertPolicy, OverlapPolicy};
use tellers_timeline_core::{
    validate_timeline, Clip, Gap, Item, MediaSource, Stack, Timeline, Track, TrackKind,
};
use uuid::Uuid;

#[pyclass(name = "MediaSource")]
#[derive(Clone)]
struct PyMediaSource {
    inner: MediaSource,
}

#[pymethods]
impl PyMediaSource {
    #[new]
    fn new(url: String) -> Self {
        Self {
            inner: MediaSource {
                otio_schema: "ExternalReference.1".to_string(),
                url,
                media_start: 0.0,
                media_duration: None,
                metadata: serde_json::Value::Null,
            },
        }
    }
    fn get_url(&self) -> String {
        self.inner.url.clone()
    }
    fn set_url(&mut self, url: String) {
        self.inner.url = url;
    }
    fn get_media_start(&self) -> f64 {
        self.inner.media_start
    }
    fn set_media_start(&mut self, v: f64) {
        self.inner.media_start = v;
    }
    fn get_media_duration(&self) -> Option<f64> {
        self.inner.media_duration
    }
    fn set_media_duration(&mut self, v: Option<f64>) {
        self.inner.media_duration = v;
    }
}

#[pyclass(name = "Clip")]
#[derive(Clone)]
struct PyClip {
    inner: Clip,
}

#[pymethods]
impl PyClip {
    #[new]
    #[pyo3(signature = (duration, source, name=None))]
    fn new(duration: f64, source: PyMediaSource, name: Option<String>) -> Self {
        Self {
            inner: Clip {
                otio_schema: "Clip.2".to_string(),
                name,
                duration,
                source: source.inner,
                metadata: serde_json::Value::Null,
            },
        }
    }
    fn get_name(&self) -> Option<String> {
        self.inner.name.clone()
    }
    fn set_name(&mut self, name: Option<String>) {
        self.inner.name = name;
    }
    fn get_duration(&self) -> f64 {
        self.inner.duration
    }
    fn set_duration(&mut self, v: f64) {
        self.inner.duration = v;
    }
    fn get_source(&self) -> PyMediaSource {
        PyMediaSource {
            inner: self.inner.source.clone(),
        }
    }
    fn set_source(&mut self, source: PyMediaSource) {
        self.inner.source = source.inner;
    }
}

#[pyclass(name = "Gap")]
#[derive(Clone)]
struct PyGap {
    inner: Gap,
}

#[pymethods]
impl PyGap {
    #[new]
    fn new(duration: f64) -> Self {
        Self {
            inner: Gap {
                otio_schema: "Gap.1".to_string(),
                duration,
                metadata: serde_json::Value::Null,
            },
        }
    }
    fn get_duration(&self) -> f64 {
        self.inner.duration
    }
    fn set_duration(&mut self, v: f64) {
        self.inner.duration = v;
    }
}

#[pyclass(name = "Item")]
#[derive(Clone)]
struct PyItem {
    inner: Item,
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
    #[new]
    fn new(kind: Option<String>) -> Self {
        let k = kind
            .map(|s| track_kind_from_str(&s))
            .unwrap_or(TrackKind::Video);
        Self {
            inner: Track {
                otio_schema: "Track.1".to_string(),
                kind: k,
                items: vec![],
                metadata: serde_json::Value::Null,
            },
        }
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
    fn clear_items(&mut self) {
        self.inner.items.clear();
    }
    fn sanitize(&mut self) {
        self.inner.sanitize();
    }
    fn append(&mut self, item: &Bound<PyAny>) -> PyResult<()> {
        if let Some(inner_item) = extract_item(item) {
            self.inner.append(inner_item);
            Ok(())
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "append expects an Item, Clip, or Gap",
            ))
        }
    }
    fn insert_at_index(
        &mut self,
        index: usize,
        item: &Bound<PyAny>,
        overlap_policy: &str,
    ) -> PyResult<()> {
        if let Some(inner_item) = extract_item(item) {
            let op = overlap_policy_from_str(overlap_policy);
            self.inner.insert_at_index(index, inner_item, op);
            Ok(())
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "insert_at_index expects an Item, Clip, or Gap",
            ))
        }
    }
    fn insert_at_time(
        &mut self,
        start_time: f64,
        item: &Bound<PyAny>,
        overlap_policy: &str,
        insert_policy: &str,
    ) -> PyResult<()> {
        if let Some(inner_item) = extract_item(item) {
            let op = overlap_policy_from_str(overlap_policy);
            let ip = insert_policy_from_str(insert_policy);
            self.inner.insert_at_time(start_time, inner_item, op, ip);
            Ok(())
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "insert_at_time expects an Item, Clip, or Gap",
            ))
        }
    }
    fn split_at_time(&mut self, time: f64) {
        self.inner.split_at_time(time);
    }
    fn get_item_at_time(&self, time: f64) -> Option<usize> {
        self.inner.get_item_at_time(time)
    }
    fn get_item_by_id(&self, py: Python<'_>, id: &str) -> Option<(usize, Py<PyItem>)> {
        let uuid = Uuid::parse_str(id).ok()?;
        self.inner.get_item_by_id(uuid).map(|(i, _it)| {
            let item = self.inner.items[i].clone();
            (i, Py::new(py, PyItem { inner: item }).unwrap())
        })
    }
    fn replace_item(&mut self, index: usize, item: &Bound<PyAny>) -> PyResult<bool> {
        if let Some(inner_item) = extract_item(item) {
            Ok(self.inner.replace_item_by_index(index, inner_item))
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "replace_item_by_index expects an Item, Clip, or Gap",
            ))
        }
    }
    fn delete_clip(&mut self, index: usize, replace_with_gap: bool) -> bool {
        self.inner.delete_clip(index, replace_with_gap)
    }
    fn resize_item(
        &mut self,
        index: usize,
        new_start_time: f64,
        new_duration: f64,
        overlap_policy: &str,
        clamp_to_media: bool,
    ) -> bool {
        let op = overlap_policy_from_str(overlap_policy);
        self.inner
            .resize_item(index, new_start_time, new_duration, op, clamp_to_media)
    }
    fn total_duration(&self) -> f64 {
        self.inner.total_duration()
    }
    fn start_time_of_item(&self, index: usize) -> f64 {
        self.inner.start_time_of_item(index)
    }
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
        let clip = Clip {
            otio_schema: "Clip.2".to_string(),
            name,
            duration,
            source: ms,
            metadata: serde_json::Value::Null,
        };
        let item = Item::Clip(clip);
        let op = overlap_policy_from_str(overlap_policy);
        let ip = insert_policy_from_str(insert_policy);
        self.inner.insert_at_time(start_time, item, op, ip);
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
    fn new() -> Self {
        Self {
            inner: Stack::default(),
        }
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
    fn add_track(&mut self, track: PyTrack) {
        self.inner.children.push(track.inner);
    }
    fn sanitize(&mut self) {
        self.inner.sanitize();
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

#[pyclass(name = "Timeline")]
#[derive(Clone)]
struct PyTimeline {
    inner: Timeline,
}

#[pymethods]
impl PyTimeline {
    #[new]
    fn new() -> Self {
        Self {
            inner: Timeline::default(),
        }
    }

    #[staticmethod]
    fn parse_json(s: &str) -> PyResult<Self> {
        let tl: Timeline = serde_json::from_str(s)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { inner: tl })
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(&self.inner)
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
    fn tracks(&self, py: Python<'_>) -> Vec<Py<PyTrack>> {
        self.inner
            .tracks
            .children
            .iter()
            .cloned()
            .map(|t| Py::new(py, PyTrack { inner: t }).unwrap())
            .collect()
    }
    fn add_track(&mut self, track: PyTrack) {
        self.inner.tracks.children.push(track.inner);
    }
    fn set_tracks(&mut self, tracks: Vec<PyTrack>) {
        self.inner.tracks = Stack {
            otio_schema: "Stack.1".to_string(),
            children: tracks.into_iter().map(|t| t.inner).collect(),
            metadata: serde_json::Value::Null,
        };
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
}

#[pymodule]
fn tellers_timeline(_py: Python, m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<PyMediaSource>()?;
    m.add_class::<PyClip>()?;
    m.add_class::<PyGap>()?;
    m.add_class::<PyItem>()?;
    m.add_class::<PyTrack>()?;
    m.add_class::<PyStack>()?;
    m.add_class::<PyTimeline>()?;
    Ok(())
}
