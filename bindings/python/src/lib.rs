use pyo3::prelude::*;
// use pyo3::types::PyList;
use pyo3::types::PyAny;
use tellers_timeline_core::to_json_with_precision;
use tellers_timeline_core::track_methods::track_item_insert::{InsertPolicy, OverlapPolicy};
use tellers_timeline_core::{
    validate_timeline, Clip, Gap, Item, MediaReference, RationalTime, Stack, TimeRange, Timeline,
    Track, TrackKind,
};
use tellers_timeline_core::{IdMetadataExt, MetadataExt};

#[pyclass(name = "MediaSource")]
#[derive(Clone)]
struct PyMediaSource {
    inner: MediaReference,
}

#[pymethods]
impl PyMediaSource {
    #[new]
    #[pyo3(signature = (url, name=None, media_start=None, media_duration=None, metadata_json=None))]
    fn new(
        url: String,
        name: Option<String>,
        media_start: Option<f64>,
        media_duration: Option<f64>,
        metadata_json: Option<String>,
    ) -> PyResult<Self> {
        let metadata = if let Some(s) = metadata_json {
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

        let mut inner = MediaReference {
            otio_schema: "ExternalReference.1".to_string(),
            target_url: url,
            available_range: None,
            name,
            available_image_bounds: None,
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
    fn get_url(&self) -> String {
        self.inner.target_url.clone()
    }
    fn set_url(&mut self, url: String) {
        self.inner.target_url = url;
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
    fn __str__(&self) -> PyResult<String> {
        to_json_with_precision(&self.inner, None, false)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
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
    #[pyo3(signature = (duration, source, name=None, id=None))]
    fn new(duration: f64, source: PyMediaSource, name: Option<String>, id: Option<String>) -> Self {
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
        let inner = Clip::new_single(sr, "DEFAULT_MEDIA".to_string(), source.inner, name, id);
        Self { inner }
    }
    fn get_name(&self) -> Option<String> {
        self.inner.name.clone()
    }
    fn set_name(&mut self, name: Option<String>) {
        self.inner.name = name;
    }
    fn get_duration(&self) -> f64 {
        self.inner.source_range.duration.value
    }
    fn set_duration(&mut self, v: f64) {
        self.inner.source_range.duration.value = v;
    }
    fn get_source(&self) -> Option<PyMediaSource> {
        let key = self
            .inner
            .active_media_reference_key
            .as_deref()
            .unwrap_or("DEFAULT_MEDIA");
        self.inner
            .media_references
            .get(key)
            .cloned()
            .map(|inner| PyMediaSource { inner })
    }
    fn set_source(&mut self, source: PyMediaSource) {
        let key = self
            .inner
            .active_media_reference_key
            .clone()
            .unwrap_or_else(|| "DEFAULT_MEDIA".to_string());
        self.inner
            .media_references
            .insert(key.clone(), source.inner);
        self.inner.active_media_reference_key = Some(key);
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
    fn __str__(&self) -> PyResult<String> {
        to_json_with_precision(&self.inner, None, false)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
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
    fn get_name(&self) -> Option<String> {
        self.inner.name.clone()
    }
    fn set_name(&mut self, name: Option<String>) {
        self.inner.name = name;
    }
    #[new]
    #[pyo3(signature = (kind=None, id=None, children=None))]
    fn new(kind: Option<String>, id: Option<String>, children: Option<Vec<PyItem>>) -> Self {
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
    fn clear_items(&mut self) {
        self.inner.items.clear();
    }
    fn set_items(&mut self, items: Vec<PyItem>) {
        self.inner.items = items.into_iter().map(|i| i.inner).collect();
    }
    fn sanitize(&mut self) {
        self.inner.sanitize();
    }
    fn split_at_time(&mut self, time: f64) {
        self.inner.split_at_time(time);
    }
    fn get_id(&self) -> Option<String> {
        self.inner.get_id()
    }
    fn set_id(&mut self, id: Option<&str>) {
        self.inner.set_id(id.map(|s| s.to_string()));
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
    fn add_track(&mut self, track: PyTrack, insertion_index: isize) {
        self.inner.add_track_at(track.inner, insertion_index);
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
    fn delete_item(
        &mut self,
        py: Python<'_>,
        id: &str,
        replace_with_gap: bool,
    ) -> Option<(usize, Py<PyItem>)> {
        self.inner
            .delete_item(id, replace_with_gap)
            .map(|(ti, it)| (ti, Py::new(py, PyItem { inner: it }).unwrap()))
    }
    fn insert_item_at_time(
        &mut self,
        dest_track_index: usize,
        dest_time: f64,
        item: &Bound<PyAny>,
        overlap_policy: &str,
        insert_policy: &str,
    ) -> PyResult<Option<String>> {
        if let Some(inner_item) = extract_item(item) {
            let op = overlap_policy_from_str(overlap_policy);
            let ip = insert_policy_from_str(insert_policy);
            Ok(self
                .inner
                .insert_item_at_time(dest_track_index, dest_time, inner_item, op, ip))
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "insert_item_at_time expects an Item, Clip, or Gap",
            ))
        }
    }
    fn insert_item_at_index(
        &mut self,
        dest_track_id: &str,
        dest_index: usize,
        item: &Bound<PyAny>,
        overlap_policy: &str,
    ) -> PyResult<Option<String>> {
        if let Some(inner_item) = extract_item(item) {
            let op = overlap_policy_from_str(overlap_policy);
            Ok(self
                .inner
                .insert_item_at_index(dest_track_id, dest_index, inner_item, op))
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "insert_item_at_index expects an Item, Clip, or Gap",
            ))
        }
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
    fn add_track(&mut self, track: PyTrack, insertion_index: isize) {
        self.inner.add_track_at(track.inner, insertion_index);
    }
    fn delete_track(&mut self, py: Python<'_>, id: &str) -> Option<Py<PyTrack>> {
        self.inner
            .delete_track(id)
            .map(|t| Py::new(py, PyTrack { inner: t }).unwrap())
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
    fn move_item(&mut self, item_id: &str, dest_track_id: &str, dest_time: f64) -> PyResult<bool> {
        // Backwards-compat convenience wrapper: default policies
        Ok(self.inner.tracks.move_item_at_time(
            item_id,
            dest_track_id,
            dest_time,
            false,
            InsertPolicy::InsertBeforeOrAfter,
            OverlapPolicy::Override,
        ))
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
    m.add_class::<PyMediaSource>()?;
    m.add_class::<PyClip>()?;
    m.add_class::<PyGap>()?;
    m.add_class::<PyItem>()?;
    m.add_class::<PyTrack>()?;
    m.add_class::<PyStack>()?;
    m.add_class::<PyTimeline>()?;
    Ok(())
}
