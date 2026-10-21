use pyo3::prelude::*;
// use pyo3::types::PyList;
use pyo3::types::{PyAny, PyDict};
use tellers_timeline_core::to_json_with_precision;
use tellers_timeline_core::track_methods::track_item_insert::{InsertPolicy, OverlapPolicy};
use tellers_timeline_core::{
    validate_timeline, Animatable, Clip, GeneratorError, ShapeParameters, TellersEffect, TextParameters, Marker, MarkerInfo, MarkerOwnerKind, MarkerUpdate, Effect, EffectMetadata, FontFace, Gap, InsertItemAtTimeResult, Item, MediaReference, MediaReferenceCrop, MediaReferencePosition, RationalTime, Stack, TextStyle, TimeRange, Timeline, TrackGroup, CompositeStack, Track, TrackKind, Transition, TransitionDirection, AudioTransitionKind,
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
    fn get_generator_kind(&self) -> Option<String> {
        self.inner.generator_kind().cloned()
    }
    fn is_text_generator(&self) -> bool {
        self.inner.is_text_generator()
    }
    fn is_shape_generator(&self) -> bool {
        self.inner.is_shape_generator()
    }
    /// A `generator_kind: "Text"` reference from a parameters dict
    /// (`text` or `words`, `maxWidth`, `wrap`, `style`).
    #[staticmethod]
    #[pyo3(signature = (parameters, name=None))]
    fn create_text_reference(parameters: &Bound<PyAny>, name: Option<String>) -> PyResult<Self> {
        let parameters: TextParameters = generator_parameters_from_python(parameters)?;
        let inner = MediaReference::new_text(&parameters, name.or_else(|| Some("Text".to_string())))
            .map_err(generator_error)?;
        Ok(Self { inner })
    }
    /// A `generator_kind: "Shape"` reference from a parameters dict.
    #[staticmethod]
    #[pyo3(signature = (parameters, name=None))]
    fn create_shape_reference(parameters: &Bound<PyAny>, name: Option<String>) -> PyResult<Self> {
        let parameters: ShapeParameters = generator_parameters_from_python(parameters)?;
        let inner = MediaReference::new_shape(&parameters, name.or_else(|| Some("Shape".to_string())))
            .map_err(generator_error)?;
        Ok(Self { inner })
    }
    fn get_text_parameters(&self, py: Python<'_>) -> PyResult<PyObject> {
        let parameters = self.inner.text_parameters().map_err(generator_error)?;
        generator_parameters_to_python(py, &parameters)
    }
    fn set_text_parameters(&mut self, parameters: &Bound<PyAny>) -> PyResult<()> {
        let parameters: TextParameters = generator_parameters_from_python(parameters)?;
        self.inner.set_text_parameters(&parameters).map_err(generator_error)
    }
    fn get_shape_parameters(&self, py: Python<'_>) -> PyResult<PyObject> {
        let parameters = self.inner.shape_parameters().map_err(generator_error)?;
        generator_parameters_to_python(py, &parameters)
    }
    fn set_shape_parameters(&mut self, parameters: &Bound<PyAny>) -> PyResult<()> {
        let parameters: ShapeParameters = generator_parameters_from_python(parameters)?;
        self.inner.set_shape_parameters(&parameters).map_err(generator_error)
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
            ..Effect::default()
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

#[pyclass(name = "MediaReferenceCrop")]
#[derive(Clone)]
struct PyMediaReferenceCrop {
    inner: MediaReferenceCrop,
}

#[pymethods]
impl PyMediaReferenceCrop {
    #[new]
    #[pyo3(signature = (crop_left=0.0, crop_right=0.0, crop_top=0.0, crop_bottom=0.0))]
    fn new(crop_left: f64, crop_right: f64, crop_top: f64, crop_bottom: f64) -> Self {
        Self {
            inner: MediaReferenceCrop {
                crop_left,
                crop_right,
                crop_top,
                crop_bottom,
            },
        }
    }
    fn get_crop_left(&self) -> f64 {
        self.inner.crop_left
    }
    fn set_crop_left(&mut self, crop_left: f64) {
        self.inner.crop_left = crop_left;
    }
    fn get_crop_right(&self) -> f64 {
        self.inner.crop_right
    }
    fn set_crop_right(&mut self, crop_right: f64) {
        self.inner.crop_right = crop_right;
    }
    fn get_crop_top(&self) -> f64 {
        self.inner.crop_top
    }
    fn set_crop_top(&mut self, crop_top: f64) {
        self.inner.crop_top = crop_top;
    }
    fn get_crop_bottom(&self) -> f64 {
        self.inner.crop_bottom
    }
    fn set_crop_bottom(&mut self, crop_bottom: f64) {
        self.inner.crop_bottom = crop_bottom;
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
    /// The speed the player plays this clip at, as `{"speed", "preserve_pitch"}`
    /// (`speed` a number, 0 for a freeze frame, or `{"keyframes": [...]}`), or
    /// None at normal speed.
    fn get_speed(&self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        self.inner
            .get_speed()
            .map(|speed| {
                let value = serde_json::json!({
                    "speed": speed.speed.to_value(),
                    "preserve_pitch": speed.preserve_pitch,
                });
                generator_parameters_to_python(py, &value)
            })
            .transpose()
    }
    /// Play this clip at a constant `speed` (0 freezes its first frame, else
    /// 0.25 to 4); `preserve_pitch=None` keeps the current setting. Raises
    /// ValueError for a speed out of range.
    #[pyo3(signature = (speed, preserve_pitch=None))]
    fn set_speed(&mut self, speed: f64, preserve_pitch: Option<bool>) -> PyResult<()> {
        self.inner
            .set_speed(speed, preserve_pitch)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
    /// Play this clip at normal speed; False when it already was.
    fn clear_speed(&mut self) -> bool {
        self.inner.clear_speed()
    }
    /// Source seconds this clip plays over its duration on the timeline.
    fn get_source_duration(&self) -> f64 {
        self.inner.source_duration()
    }
    fn get_crop(&self, py: Python<'_>) -> Py<PyMediaReferenceCrop> {
        let crop = self.inner.get_crop();
        Py::new(py, PyMediaReferenceCrop { inner: crop }).unwrap()
    }
    fn set_crop(&mut self, crop: PyRef<PyMediaReferenceCrop>) {
        self.inner.set_crop(crop.inner.clone());
    }
    /// A clip playing a Text generator built from a parameters dict.
    #[staticmethod]
    #[pyo3(signature = (duration, parameters, name=None, id=None))]
    fn new_text(
        duration: f64,
        parameters: &Bound<PyAny>,
        name: Option<String>,
        id: Option<String>,
    ) -> PyResult<Self> {
        let parameters: TextParameters = generator_parameters_from_python(parameters)?;
        let inner = Clip::new_text(duration, &parameters, name, id).map_err(generator_error)?;
        Ok(Self { inner })
    }
    /// A clip playing a Shape generator built from a parameters dict.
    #[staticmethod]
    #[pyo3(signature = (duration, parameters, name=None, id=None))]
    fn new_shape(
        duration: f64,
        parameters: &Bound<PyAny>,
        name: Option<String>,
        id: Option<String>,
    ) -> PyResult<Self> {
        let parameters: ShapeParameters = generator_parameters_from_python(parameters)?;
        let inner = Clip::new_shape(duration, &parameters, name, id).map_err(generator_error)?;
        Ok(Self { inner })
    }
    fn is_text_clip(&self) -> bool {
        self.inner.is_text_clip()
    }
    fn is_shape_clip(&self) -> bool {
        self.inner.is_shape_clip()
    }
    fn get_text_parameters(&self, py: Python<'_>) -> PyResult<PyObject> {
        let parameters = self.inner.text_parameters().map_err(generator_error)?;
        generator_parameters_to_python(py, &parameters)
    }
    fn set_text_parameters(&mut self, parameters: &Bound<PyAny>) -> PyResult<()> {
        let parameters: TextParameters = generator_parameters_from_python(parameters)?;
        self.inner.set_text_parameters(&parameters).map_err(generator_error)
    }
    fn get_shape_parameters(&self, py: Python<'_>) -> PyResult<PyObject> {
        let parameters = self.inner.shape_parameters().map_err(generator_error)?;
        generator_parameters_to_python(py, &parameters)
    }
    fn set_shape_parameters(&mut self, parameters: &Bound<PyAny>) -> PyResult<()> {
        let parameters: ShapeParameters = generator_parameters_from_python(parameters)?;
        self.inner.set_shape_parameters(&parameters).map_err(generator_error)
    }
    /// The string a Text clip shows: `text`, else its words joined; None for other clips.
    fn get_text(&self) -> Option<String> {
        self.inner.get_text()
    }
    /// Sets a Text clip's string, dropping its word timings.
    fn set_text(&mut self, text: String) -> PyResult<()> {
        self.inner.set_text(text).map_err(generator_error)
    }
    /// The tellers effects (filters, Transform, Mask, Volume, Text effects, ...) of this clip,
    /// in order, as `{"effect_name", "name", "enabled", "parameters"}` dicts.
    fn get_tellers_effects(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        tellers_effects_to_python(py, self.inner.get_tellers_effects())
    }
    /// Replace every tellers effect of this clip with `effects` (dicts like
    /// `get_tellers_effects` returns); other effects stay. False, changing
    /// nothing, when an `effect_name` is unknown.
    fn set_tellers_effects(&mut self, effects: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.set_tellers_effects(tellers_effects_from_python(effects)?))
    }
    /// Append a tellers effect to this clip; False when its `effect_name` is unknown.
    fn push_tellers_effect(&mut self, effect: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.push_tellers_effect(tellers_effect_from_python(effect)?))
    }
    /// Insert a tellers effect into this clip at `index` (counting tellers effects only).
    fn insert_tellers_effect_at(&mut self, index: usize, effect: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.insert_tellers_effect_at(index, tellers_effect_from_python(effect)?))
    }
    /// Replace the tellers effect of this clip at `index`, keeping its other keys;
    /// returns the previous one, or None when out of range.
    fn replace_tellers_effect_at(&mut self, py: Python<'_>, index: usize, effect: &Bound<PyAny>) -> PyResult<Option<PyObject>> {
        let effect = tellers_effect_from_python(effect)?;
        self.inner
            .replace_tellers_effect_at(index, effect)
            .map(|e| tellers_effect_to_python(py, &e))
            .transpose()
    }
    /// Remove and return the tellers effect of this clip at `index`.
    fn remove_tellers_effect_at(&mut self, py: Python<'_>, index: usize) -> PyResult<Option<PyObject>> {
        self.inner
            .remove_tellers_effect_at(index)
            .map(|e| tellers_effect_to_python(py, &e))
            .transpose()
    }
    /// Remove every tellers effect of this clip, returning whether any was present.
    fn clear_tellers_effects(&mut self) -> bool {
        self.inner.clear_tellers_effects()
    }
    /// Turn the tellers effect of this clip at `index` on or off.
    fn set_tellers_effect_enabled(&mut self, index: usize, enabled: bool) -> bool {
        self.inner.set_tellers_effect_enabled(index, enabled)
    }
    /// Set the parameter `key` of the tellers effect of this clip at `index`
    /// to `value` (a number, `{"keyframes": [...]}`, a string or a bool);
    /// None removes it.
    #[pyo3(signature = (index, key, value=None))]
    fn set_tellers_effect_parameter(&mut self, index: usize, key: &str, value: Option<&Bound<PyAny>>) -> PyResult<bool> {
        let value = value.filter(|v| !v.is_none()).map(json_from_python).transpose()?;
        Ok(self.inner.set_tellers_effect_parameter(index, key, value))
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

#[pyclass(name = "Transition")]
#[derive(Clone)]
struct PyTransition {
    inner: Transition,
}

#[pymethods]
impl PyTransition {
    /// An OTIO transition between two neighbouring items. Offsets are in seconds:
    /// `in_offset` reaches back into the previous item, `out_offset` into the next.
    #[new]
    #[pyo3(signature = (in_offset, out_offset, transition_type=None, id=None))]
    fn new(in_offset: f64, out_offset: f64, transition_type: Option<String>, id: Option<String>) -> Self {
        Self {
            inner: Transition::new(in_offset, out_offset, transition_type, id),
        }
    }
    #[staticmethod]
    fn parse_json(json_str: &str) -> PyResult<Self> {
        let inner: Transition = serde_json::from_str(json_str)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { inner })
    }
    fn to_json(&self) -> PyResult<String> {
        to_json_with_precision(&self.inner, None, false)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }
    fn get_name(&self) -> Option<String> {
        self.inner.name.clone()
    }
    fn set_name(&mut self, name: Option<String>) {
        self.inner.name = name;
    }
    fn get_transition_type(&self) -> String {
        self.inner.transition_type.clone()
    }
    fn set_transition_type(&mut self, transition_type: String) {
        self.inner.transition_type = transition_type;
    }
    /// How the player draws it: "Dissolve", "DipToColor", "Wipe", "Push", "Slide" or "Zoom".
    fn get_kind(&self) -> &'static str {
        self.inner.kind().as_str()
    }
    /// Whether it is an audio transition: its transition_type is a "tellers.ai.audio-<name>".
    fn is_audio(&self) -> bool {
        self.inner.is_audio()
    }
    /// Whether it goes on a track of `kind` ("video", "audio" or "other"): an audio
    /// transition on audio tracks only, any other on any other track.
    fn allowed_on(&self, kind: &str) -> PyResult<bool> {
        let kind = match kind.to_ascii_lowercase().as_str() {
            "video" => TrackKind::Video,
            "audio" => TrackKind::Audio,
            "other" => TrackKind::Other,
            _ => return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("unknown track kind: {kind}"))),
        };
        Ok(self.inner.allowed_on(&kind))
    }
    /// Writes a kind name (read like the player, e.g. "cross dissolve"); "DipToBlack" and
    /// "DipToWhite" also write their colour.
    fn set_kind(&mut self, kind: &str) -> PyResult<()> {
        if self.inner.set_kind_named(kind) {
            Ok(())
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("unknown transition kind: {kind}")))
        }
    }
    /// The audio fade curve of a "tellers.ai.audio-<name>" type: "constant-power" or
    /// "constant-gain"; None for any other type.
    fn get_audio_kind(&self) -> Option<&'static str> {
        self.inner.audio_kind().map(AudioTransitionKind::as_str)
    }
    /// Writes "tellers.ai.audio-<name>" as the transition_type; takes the name or the full type.
    fn set_audio_kind(&mut self, kind: &str) -> PyResult<()> {
        let kind = AudioTransitionKind::parse(kind).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("unknown audio transition kind: {kind}"))
        })?;
        self.inner.set_audio_kind(kind);
        Ok(())
    }
    /// "left", "right", "up" or "down"; "left" when unset.
    fn get_direction(&self) -> &'static str {
        self.inner.direction().as_str()
    }
    #[pyo3(signature = (direction=None))]
    fn set_direction(&mut self, direction: Option<&str>) -> PyResult<()> {
        let direction = direction
            .map(|d| {
                TransitionDirection::parse(d).ok_or_else(|| {
                    PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("unknown transition direction: {d}"))
                })
            })
            .transpose()?;
        self.inner.set_direction(direction);
        Ok(())
    }
    /// The colour a dip goes through; "#000000" when unset.
    fn get_color(&self) -> String {
        self.inner.color()
    }
    #[pyo3(signature = (color=None))]
    fn set_color(&mut self, color: Option<&str>) {
        self.inner.set_color(color);
    }
    fn get_in_offset(&self) -> f64 {
        self.inner.in_offset_seconds()
    }
    fn set_in_offset(&mut self, seconds: f64) {
        self.inner.in_offset.set_from_seconds(seconds);
    }
    fn get_out_offset(&self) -> f64 {
        self.inner.out_offset_seconds()
    }
    fn set_out_offset(&mut self, seconds: f64) {
        self.inner.out_offset.set_from_seconds(seconds);
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
        self.to_json()
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
    /// A composite clip (a nested `Stack`) made of `children` tracks. It lasts
    /// as long as its longest track until a source range is set.
    #[staticmethod]
    #[pyo3(signature = (children, name=None, id=None))]
    fn from_composite(children: Vec<PyTrack>, name: Option<String>, id: Option<String>) -> Self {
        Self {
            inner: Item::Composite(CompositeStack::new(
                children.into_iter().map(|t| t.inner).collect(),
                name,
                id,
            )),
        }
    }
    #[staticmethod]
    fn from_transition(t: PyTransition) -> Self {
        Self {
            inner: Item::Transition(t.inner),
        }
    }
    fn is_clip(&self) -> bool {
        matches!(self.inner, Item::Clip(_))
    }
    fn is_gap(&self) -> bool {
        matches!(self.inner, Item::Gap(_))
    }
    fn is_composite(&self) -> bool {
        matches!(self.inner, Item::Composite(_))
    }
    /// The tracks of a composite clip; empty for clips and gaps.
    fn get_children(&self, py: Python<'_>) -> Vec<Py<PyTrack>> {
        let Item::Composite(stack) = &self.inner else {
            return Vec::new();
        };
        stack
            .children
            .iter()
            .map(|t| Py::new(py, PyTrack { inner: t.clone() }).unwrap())
            .collect()
    }
    fn is_transition(&self) -> bool {
        self.inner.is_transition()
    }
    /// The transition payload when this item is one, else `None`.
    fn as_transition(&self) -> Option<PyTransition> {
        match &self.inner {
            Item::Transition(t) => Some(PyTransition { inner: t.clone() }),
            _ => None,
        }
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
    /// The tellers effects (filters, Transform, Blend, Mask, ...) of this clip or composite clip
    /// (none for a gap), in order, as `{"effect_name", "name", "enabled", "parameters"}` dicts.
    /// Items returned by a timeline are copies: use the
    /// `Timeline.*_item_tellers_effect*` methods to change one in it.
    fn get_tellers_effects(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        tellers_effects_to_python(py, self.inner.get_tellers_effects())
    }
    /// Replace every tellers effect of this clip or composite clip with `effects` (dicts like
    /// `get_tellers_effects` returns); other effects stay. False, changing
    /// nothing, when an `effect_name` is unknown or this is a gap.
    fn set_tellers_effects(&mut self, effects: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.set_tellers_effects(tellers_effects_from_python(effects)?))
    }
    /// Append a tellers effect to this clip or composite clip; False when its `effect_name` is unknown.
    fn push_tellers_effect(&mut self, effect: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.push_tellers_effect(tellers_effect_from_python(effect)?))
    }
    /// Insert a tellers effect into this clip or composite clip at `index` (counting tellers effects only).
    fn insert_tellers_effect_at(&mut self, index: usize, effect: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.insert_tellers_effect_at(index, tellers_effect_from_python(effect)?))
    }
    /// Replace the tellers effect of this clip or composite clip at `index`, keeping its other keys;
    /// returns the previous one, or None when out of range.
    fn replace_tellers_effect_at(&mut self, py: Python<'_>, index: usize, effect: &Bound<PyAny>) -> PyResult<Option<PyObject>> {
        let effect = tellers_effect_from_python(effect)?;
        self.inner
            .replace_tellers_effect_at(index, effect)
            .map(|e| tellers_effect_to_python(py, &e))
            .transpose()
    }
    /// Remove and return the tellers effect of this clip or composite clip at `index`.
    fn remove_tellers_effect_at(&mut self, py: Python<'_>, index: usize) -> PyResult<Option<PyObject>> {
        self.inner
            .remove_tellers_effect_at(index)
            .map(|e| tellers_effect_to_python(py, &e))
            .transpose()
    }
    /// Remove every tellers effect of this clip or composite clip, returning whether any was present.
    fn clear_tellers_effects(&mut self) -> bool {
        self.inner.clear_tellers_effects()
    }
    /// Turn the tellers effect of this clip or composite clip at `index` on or off.
    fn set_tellers_effect_enabled(&mut self, index: usize, enabled: bool) -> bool {
        self.inner.set_tellers_effect_enabled(index, enabled)
    }
    /// Set the parameter `key` of the tellers effect of this clip or composite clip at `index`
    /// to `value` (a number, `{"keyframes": [...]}`, a string or a bool);
    /// None removes it.
    #[pyo3(signature = (index, key, value=None))]
    fn set_tellers_effect_parameter(&mut self, index: usize, key: &str, value: Option<&Bound<PyAny>>) -> PyResult<bool> {
        let value = value.filter(|v| !v.is_none()).map(json_from_python).transpose()?;
        Ok(self.inner.set_tellers_effect_parameter(index, key, value))
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
    fn get_crop(&self, py: Python<'_>) -> Py<PyMediaReferenceCrop> {
        let crop = self.inner.get_crop();
        Py::new(py, PyMediaReferenceCrop { inner: crop }).unwrap()
    }
    fn set_crop(&mut self, crop: PyRef<PyMediaReferenceCrop>) {
        self.inner.set_crop(crop.inner.clone());
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
    /// The CSS classes declared on this track only (not merged with the
    /// timeline's), as `{"name", "declarations"}` dicts.
    fn get_text_styles(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        text_styles_to_python(py, self.inner.get_text_styles())
    }
    /// The declaration block of the class `name` declared on this track only.
    fn get_text_style(&self, name: &str) -> Option<String> {
        self.inner.get_text_style(name)
    }
    /// Declare (or redefine) the class `name` on this track object. Tracks
    /// returned by a timeline are copies: use
    /// `Timeline.set_track_text_style` to change a track already in one.
    fn set_text_style(&mut self, name: String, declarations: String) -> bool {
        self.inner.set_text_style(name, declarations)
    }
    /// Remove the class `name` from this track object.
    fn remove_text_style(&mut self, name: &str) -> bool {
        self.inner.remove_text_style(name)
    }
    /// The tellers effects (filters, Transform, Mask, Volume, Text effects, ...) of this track object, applied to each of its clips. Tracks returned by a timeline are copies: use the `Timeline.*_track_tellers_effect*` methods to change one in it,
    /// in order, as `{"effect_name", "name", "enabled", "parameters"}` dicts.
    fn get_tellers_effects(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        tellers_effects_to_python(py, self.inner.get_tellers_effects())
    }
    /// Replace every tellers effect of this track object, applied to each of its clips. Tracks returned by a timeline are copies: use the `Timeline.*_track_tellers_effect*` methods to change one in it with `effects` (dicts like
    /// `get_tellers_effects` returns); other effects stay. False, changing
    /// nothing, when an `effect_name` is unknown or an effect does not go on this kind of track (no Volume or Pan on a video track, no picture or text effect on an audio one).
    fn set_tellers_effects(&mut self, effects: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.set_tellers_effects(tellers_effects_from_python(effects)?))
    }
    /// Append a tellers effect to this track object, applied to each of its clips. Tracks returned by a timeline are copies: use the `Timeline.*_track_tellers_effect*` methods to change one in it; False when its `effect_name` is unknown or it does not go on this kind of track.
    fn push_tellers_effect(&mut self, effect: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.push_tellers_effect(tellers_effect_from_python(effect)?))
    }
    /// Insert a tellers effect into this track object, applied to each of its clips. Tracks returned by a timeline are copies: use the `Timeline.*_track_tellers_effect*` methods to change one in it at `index` (counting tellers effects only).
    fn insert_tellers_effect_at(&mut self, index: usize, effect: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.insert_tellers_effect_at(index, tellers_effect_from_python(effect)?))
    }
    /// Replace the tellers effect of this track object, applied to each of its clips. Tracks returned by a timeline are copies: use the `Timeline.*_track_tellers_effect*` methods to change one in it at `index`, keeping its other keys;
    /// returns the previous one, or None when out of range.
    fn replace_tellers_effect_at(&mut self, py: Python<'_>, index: usize, effect: &Bound<PyAny>) -> PyResult<Option<PyObject>> {
        let effect = tellers_effect_from_python(effect)?;
        self.inner
            .replace_tellers_effect_at(index, effect)
            .map(|e| tellers_effect_to_python(py, &e))
            .transpose()
    }
    /// Remove and return the tellers effect of this track object, applied to each of its clips. Tracks returned by a timeline are copies: use the `Timeline.*_track_tellers_effect*` methods to change one in it at `index`.
    fn remove_tellers_effect_at(&mut self, py: Python<'_>, index: usize) -> PyResult<Option<PyObject>> {
        self.inner
            .remove_tellers_effect_at(index)
            .map(|e| tellers_effect_to_python(py, &e))
            .transpose()
    }
    /// Remove every tellers effect of this track object, applied to each of its clips. Tracks returned by a timeline are copies: use the `Timeline.*_track_tellers_effect*` methods to change one in it, returning whether any was present.
    fn clear_tellers_effects(&mut self) -> bool {
        self.inner.clear_tellers_effects()
    }
    /// Turn the tellers effect of this track object, applied to each of its clips. Tracks returned by a timeline are copies: use the `Timeline.*_track_tellers_effect*` methods to change one in it at `index` on or off.
    fn set_tellers_effect_enabled(&mut self, index: usize, enabled: bool) -> bool {
        self.inner.set_tellers_effect_enabled(index, enabled)
    }
    /// Set the parameter `key` of the tellers effect of this track object, applied to each of its clips. Tracks returned by a timeline are copies: use the `Timeline.*_track_tellers_effect*` methods to change one in it at `index`
    /// to `value` (a number, `{"keyframes": [...]}`, a string or a bool);
    /// None removes it.
    #[pyo3(signature = (index, key, value=None))]
    fn set_tellers_effect_parameter(&mut self, index: usize, key: &str, value: Option<&Bound<PyAny>>) -> PyResult<bool> {
        let value = value.filter(|v| !v.is_none()).map(json_from_python).transpose()?;
        Ok(self.inner.set_tellers_effect_parameter(index, key, value))
    }
    /// Rename the track; this clears its auto-created mark.
    fn set_name(&mut self, name: Option<String>) {
        self.inner.set_name(name);
    }
    /// Whether the library created this track (a pruning candidate).
    fn is_auto_created(&self) -> bool {
        self.inner.is_auto_created()
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
    /// The associated track ids of the track with `track_id`, nearest first,
    /// computed from the sync clips. None when the track is unknown.
    fn associated_track_ids(&self, track_id: &str) -> Option<Vec<String>> {
        self.inner.associated_track_ids(track_id)
    }
    /// An existing audio track free over `[start, start + duration]` for a clip
    /// without sync partners: tracks no track lists as a partner first, then
    /// the lowest index. None when no audio track is free there.
    fn free_audio_track_for(&self, start: f64, duration: f64) -> Option<String> {
        self.inner.free_audio_track_for(start, duration)
    }
    /// Reorder the tracks so every group is contiguous: owner on top, its
    /// partners right below in list order (see `associated_track_ids`). Call
    /// it once at the end of a request: it is the only library call that
    /// changes track indices. Returns whether the order changed.
    fn normalize_track_order(&mut self) -> bool {
        self.inner.normalize_track_order()
    }
    /// Remove partner tracks with no clip and no user name (auto-numbered
    /// tracks the library created). Owners, free tracks and named tracks
    /// stay. Returns the removed track ids. A separate step, like
    /// `normalize_track_order`.
    fn prune_empty_partner_tracks(&mut self) -> Vec<String> {
        self.inner.prune_empty_partner_tracks()
    }
    /// Every track group, owners in stack order (bottom to top): the owner,
    /// its kind ("video" or "audio" group) and its partners in slot order,
    /// computed from the sync clips. Tracks in no group are groups of one, so
    /// every track appears exactly once.
    fn track_groups(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        track_groups_to_python(py, self.inner.track_groups())
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
    /// The one track named `name`, or None when no track — or several tracks —
    /// carry that name.
    fn get_track_by_name(&self, py: Python<'_>, name: &str) -> Option<(usize, Py<PyTrack>)> {
        self.inner.get_track_by_name(name).map(|(i, _t)| {
            let tr = self.inner.children[i].clone();
            (i, Py::new(py, PyTrack { inner: tr }).unwrap())
        })
    }
    /// Every track named `name`, in stack order. Names are display labels and
    /// are not required to be unique.
    fn find_tracks_by_name(&self, py: Python<'_>, name: &str) -> Vec<(usize, Py<PyTrack>)> {
        self.inner
            .find_tracks_by_name(name)
            .into_iter()
            .map(|(i, tr)| (i, Py::new(py, PyTrack { inner: tr.clone() }).unwrap()))
            .collect()
    }
    fn get_item(&self, py: Python<'_>, id: &str) -> Option<(usize, usize, Py<PyItem>)> {
        self.inner.get_item(id).map(|(ti, ii, _it)| {
            let item = self.inner.children[ti].items[ii].clone();
            (ti, ii, Py::new(py, PyItem { inner: item }).unwrap())
        })
    }
    /// The item `id` at any depth, inside composite clips too (`get_item` sees
    /// top-level items only).
    fn get_nested_item(&self, py: Python<'_>, id: &str) -> Option<Py<PyItem>> {
        let item = self.inner.get_nested_item(id)?.clone();
        Some(Py::new(py, PyItem { inner: item }).unwrap())
    }
    /// The ids of the composite clips enclosing the item `id`, outermost
    /// first: `[]` for a top-level item, None when there is no such item. Its
    /// last entry is the composite to pass to `edit_composite` to edit the item.
    fn composite_path(&self, id: &str) -> Option<Vec<String>> {
        self.inner.composite_path(id)
    }
    /// A copy of the tracks of the composite clip `composite_id` (at any
    /// depth) as a Stack named after it; None when there is no such composite.
    fn composite_stack(&self, py: Python<'_>, composite_id: &str) -> Option<Py<PyStack>> {
        let composite = self.inner.get_composite(composite_id)?;
        let inner = Stack {
            name: composite.name.clone(),
            children: composite.children.clone(),
            ..Stack::default()
        };
        Some(Py::new(py, PyStack { inner }).unwrap())
    }
    /// Call `edit(stack)` with the tracks of the composite clip `composite_id`
    /// (at any depth) as a working Stack, so item ops apply inside it, and
    /// return what `edit` returns. The tracks `edit` leaves become the
    /// composite's; the stack is only valid during the call. When `edit`
    /// raises, the composite is left unchanged and the error propagates.
    /// A composite without source_range follows its content's length and the
    /// enclosing tracks ripple (a resize of its tail with "push"); emptying
    /// one removes it. Raises KeyError when there is no such composite.
    fn edit_composite(
        &mut self,
        py: Python<'_>,
        composite_id: &str,
        edit: &Bound<PyAny>,
    ) -> PyResult<PyObject> {
        let outcome = self.inner.with_composite_mut(composite_id, |stack| {
            let original = stack.children.clone();
            let working = Py::new(py, PyStack { inner: std::mem::take(stack) }).unwrap();
            let result = edit.call1((working.clone_ref(py),));
            let mut working = working.borrow_mut(py);
            *stack = std::mem::take(&mut working.inner);
            if result.is_err() {
                stack.children = original;
            }
            result.map(Bound::unbind)
        });
        match outcome {
            Some(result) => result,
            None => Err(PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!(
                "no composite clip {composite_id:?}"
            ))),
        }
    }
    /// Replace the tracks of the composite clip `composite_id` (at any depth),
    /// rippling as `edit_composite` does; False when there is no such composite.
    fn set_composite_tracks(&mut self, composite_id: &str, tracks: Vec<PyTrack>) -> bool {
        self.inner
            .set_composite_tracks(composite_id, tracks.into_iter().map(|t| t.inner).collect())
    }
    #[pyo3(signature = (id, replace_with_gap, with_sync_clips=true))]
    fn delete_item(
        &mut self,
        py: Python<'_>,
        id: &str,
        replace_with_gap: bool,
        with_sync_clips: bool,
    ) -> PyResult<Vec<(usize, Py<PyItem>)>> {
        if !replace_with_gap && !with_sync_clips {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "with_sync_clips=False requires replace_with_gap=True",
            ));
        }
        Ok(self
            .inner
            .delete_item(id, replace_with_gap, with_sync_clips)
            .into_iter()
            .map(|(ti, it)| (ti, Py::new(py, PyItem { inner: it }).unwrap()))
            .collect())
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
    #[pyo3(signature = (dest_track_id, dest_time, item, overlap_policy, insert_policy, linked_audio_clips=None, linked_video_clip=None))]
    fn insert_item_at_time_by_id(
        &mut self,
        py: Python<'_>,
        dest_track_id: &str,
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
            match self.inner.insert_item_at_time_by_id(
                dest_track_id,
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
    /// `Clip.set_speed` on the clip `item_id` and, with `with_sync_clips`, on
    /// its sync partners. Returns how many clips changed (0 when no clip has
    /// this id); raises ValueError for a speed out of range.
    #[pyo3(signature = (item_id, speed, preserve_pitch=None, with_sync_clips=true))]
    fn set_item_speed(
        &mut self,
        item_id: &str,
        speed: f64,
        preserve_pitch: Option<bool>,
        with_sync_clips: bool,
    ) -> PyResult<usize> {
        self.inner
            .set_item_speed(item_id, speed, preserve_pitch, with_sync_clips)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
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
    /// Resize by source range, as the app editor's clip handles do: `resize_from_start`
    /// trims the head, `push_following` ripples the rest of the track (else a gap stays).
    fn modify_item(
        &mut self,
        item_id: &str,
        source_start_time: f64,
        duration: f64,
        clamp_to_media: bool,
        resize_from_start: bool,
        push_following: bool,
    ) -> bool {
        self.inner.modify_item(
            item_id,
            source_start_time,
            duration,
            clamp_to_media,
            resize_from_start,
            push_following,
        )
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
    if let Ok(py_transition) = item.extract::<PyRef<PyTransition>>() {
        return Some(Item::Transition(py_transition.inner.clone()));
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

fn track_groups_to_python(py: Python<'_>, groups: Vec<TrackGroup>) -> PyResult<Vec<PyObject>> {
    groups
        .into_iter()
        .map(|group| {
            let dict = PyDict::new(py);
            dict.set_item("owner_track_index", group.owner_track_index)?;
            dict.set_item("owner_track_id", group.owner_track_id)?;
            dict.set_item(
                "kind",
                match group.kind {
                    TrackKind::Video => "video",
                    TrackKind::Audio => "audio",
                    TrackKind::Other => "other",
                },
            )?;
            dict.set_item("partner_track_indices", group.partner_track_indices)?;
            dict.set_item("partner_track_ids", group.partner_track_ids)?;
            Ok(dict.into_py(py))
        })
        .collect()
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

fn font_faces_to_python(py: Python<'_>, fonts: Vec<FontFace>) -> PyResult<Vec<PyObject>> {
    fonts
        .into_iter()
        .map(|font| {
            let dict = PyDict::new(py);
            dict.set_item("family", font.family)?;
            dict.set_item("url", font.url)?;
            dict.set_item("weight", font.weight)?;
            dict.set_item("style", font.style)?;
            Ok(dict.into_py(py))
        })
        .collect()
}

fn value_error(message: String) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyValueError, _>(message)
}

fn clip_group_to_python(py: Python<'_>, group: &tellers_timeline_core::ClipGroup) -> PyResult<PyObject> {
    generator_parameters_to_python(py, &serde_json::json!({"id": group.id, "parameters": group.parameters}))
}

fn clip_group_from_python(group: &Bound<PyAny>) -> PyResult<tellers_timeline_core::ClipGroup> {
    let value: serde_json::Value = generator_parameters_from_python(group)?;
    let id = value.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let parameters = value.get("parameters").and_then(|v| v.as_object()).cloned().unwrap_or_default();
    Ok(tellers_timeline_core::ClipGroup { id, parameters })
}

fn generator_error(error: GeneratorError) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyValueError, _>(error.to_string())
}

/// Generator parameters as the plain dict of their JSON (player key names).
fn generator_parameters_to_python<T: serde::Serialize>(py: Python<'_>, parameters: &T) -> PyResult<PyObject> {
    let text = serde_json::to_string(parameters)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
    Ok(py.import_bound("json")?.call_method1("loads", (text,))?.unbind())
}

/// A JSON-like dict as typed generator parameters.
fn generator_parameters_from_python<T: serde::de::DeserializeOwned>(parameters: &Bound<PyAny>) -> PyResult<T> {
    let text: String = parameters
        .py()
        .import_bound("json")?
        .call_method1("dumps", (parameters,))?
        .extract()?;
    serde_json::from_str(&text).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("invalid generator parameters: {e}"))
    })
}

fn text_styles_to_python(py: Python<'_>, styles: Vec<TextStyle>) -> PyResult<Vec<PyObject>> {
    styles
        .into_iter()
        .map(|style| {
            let dict = PyDict::new(py);
            dict.set_item("name", style.name)?;
            dict.set_item("declarations", style.declarations)?;
            Ok(dict.into_py(py))
        })
        .collect()
}

fn marker_info_to_python(py: Python<'_>, info: MarkerInfo) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("id", info.marker.get_id())?;
    dict.set_item("name", &info.marker.name)?;
    dict.set_item("comment", &info.marker.comment)?;
    dict.set_item("color", &info.marker.color)?;
    dict.set_item("created_by", info.marker.get_created_by())?;
    dict.set_item(
        "owner_kind",
        match info.owner_kind {
            MarkerOwnerKind::Stack => "stack",
            MarkerOwnerKind::Track => "track",
            MarkerOwnerKind::Clip => "clip",
            MarkerOwnerKind::Gap => "gap",
            MarkerOwnerKind::Composite => "composite",
        },
    )?;
    dict.set_item("owner_id", info.owner_id)?;
    dict.set_item("track_id", info.track_id)?;
    dict.set_item("timeline_start", info.timeline_start)?;
    dict.set_item("duration", info.duration)?;
    dict.set_item("visible", info.visible)?;
    Ok(dict.into_py(py))
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

    /// Fold every clip's legacy Resolve volume into `tellers.ai.Volume`;
    /// returns how many Resolve effects were removed.
    fn upgrade_legacy_resolve_volumes(&mut self) -> usize {
        self.inner.upgrade_legacy_resolve_volumes()
    }

    /// Clear the `target_url` of every media reference and every effect.
    fn clear_target_urls(&mut self) {
        self.inner.clear_target_urls();
    }

    /// Every distinct asset id referenced by an effect's `asset_id`
    /// parameter (e.g. a `ColorLut`'s `.cube`), in timeline order: the ids
    /// to resolve for `set_effect_target_urls`.
    fn effect_asset_ids(&self) -> Vec<String> {
        self.inner.effect_asset_ids()
    }

    /// Set the `target_url` parameter of every effect whose `asset_id` is in
    /// `urls` (asset id -> URL), on every stage. Returns how many were set.
    fn set_effect_target_urls(&mut self, urls: std::collections::HashMap<String, String>) -> usize {
        self.inner.set_effect_target_urls(&urls)
    }

    /// `set_effect_target_urls` for the single asset `asset_id`.
    fn set_effect_target_url(&mut self, asset_id: &str, url: &str) -> usize {
        self.inner.set_effect_target_url(asset_id, url)
    }

    /// Remove the `target_url` parameter of every effect, keeping the asset
    /// ids. Returns how many had one. `clear_target_urls` does this too.
    fn clear_effect_target_urls(&mut self) -> usize {
        self.inner.clear_effect_target_urls()
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
    /// The associated track ids of the track with `track_id`, nearest first,
    /// computed from the sync clips. None when the track is unknown.
    fn associated_track_ids(&self, track_id: &str) -> Option<Vec<String>> {
        self.inner.tracks.associated_track_ids(track_id)
    }
    /// An existing audio track free over `[start, start + duration]` for a clip
    /// without sync partners: tracks no track lists as a partner first, then
    /// the lowest index. None when no audio track is free there.
    fn free_audio_track_for(&self, start: f64, duration: f64) -> Option<String> {
        self.inner.tracks.free_audio_track_for(start, duration)
    }
    /// Reorder the tracks so every group is contiguous: owner on top, its
    /// partners right below in list order (see `associated_track_ids`). Call
    /// it once at the end of a request: it is the only library call that
    /// changes track indices. Returns whether the order changed.
    fn normalize_track_order(&mut self) -> bool {
        self.inner.tracks.normalize_track_order()
    }
    /// Remove partner tracks with no clip and no user name (auto-numbered
    /// tracks the library created). Owners, free tracks and named tracks
    /// stay. Returns the removed track ids. A separate step, like
    /// `normalize_track_order`.
    fn prune_empty_partner_tracks(&mut self) -> Vec<String> {
        self.inner.tracks.prune_empty_partner_tracks()
    }
    /// Every track group, owners in stack order (bottom to top): the owner,
    /// its kind ("video" or "audio" group) and its partners in slot order,
    /// computed from the sync clips. Tracks in no group are groups of one, so
    /// every track appears exactly once.
    fn track_groups(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        track_groups_to_python(py, self.inner.tracks.track_groups())
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
    /// The custom fonts declared for rich-text clips, as
    /// `{"family", "url", "weight", "style"}` dicts. Fonts the player would
    /// ignore are skipped.
    fn get_fonts(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        font_faces_to_python(py, self.inner.get_fonts())
    }
    /// Declare a custom font, replacing an entry with the same family
    /// (case-insensitive), weight and style. Returns False, changing nothing,
    /// when the family is empty or the URL is not http/https/data.
    #[pyo3(signature = (family, url, weight=None, style=None))]
    fn add_font(
        &mut self,
        family: String,
        url: String,
        weight: Option<String>,
        style: Option<String>,
    ) -> bool {
        self.inner.add_font(FontFace {
            family,
            url,
            weight,
            style,
        })
    }
    /// Remove every declared font with this family (case-insensitive),
    /// returning whether one was present.
    fn remove_font(&mut self, family: &str) -> bool {
        self.inner.remove_font(family)
    }
    /// The CSS classes declared for rich-text clips, as
    /// `{"name", "declarations"}` dicts. Styles the player would ignore are
    /// skipped.
    fn get_text_styles(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        text_styles_to_python(py, self.inner.get_text_styles())
    }
    /// The declaration block of the class `name`, or None if not declared.
    fn get_text_style(&self, name: &str) -> Option<String> {
        self.inner.get_text_style(name)
    }
    /// Declare (or redefine) the CSS class `name` with `declarations`
    /// (`"font-size:56px;color:#fff"`), for clips to use as
    /// `class="name"` in their Title HTML. Returns False, changing nothing,
    /// when the name is not a plain CSS identifier or the declarations are
    /// empty or contain a brace.
    fn set_text_style(&mut self, name: String, declarations: String) -> bool {
        self.inner.set_text_style(name, declarations)
    }
    /// Remove the declared class `name`, returning whether it was present.
    fn remove_text_style(&mut self, name: &str) -> bool {
        self.inner.remove_text_style(name)
    }
    /// Every marker of the timeline (stack, tracks, clips) as dicts with
    /// timeline times: `{"id", "name", "comment", "color", "created_by",
    /// "owner_kind", "owner_id", "track_id", "timeline_start", "duration",
    /// "visible"}`.
    fn get_markers(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        self.inner
            .get_markers()
            .into_iter()
            .map(|info| marker_info_to_python(py, info))
            .collect()
    }
    /// The marker with id `marker_id`, as in `get_markers`, or None.
    fn get_marker(&self, py: Python<'_>, marker_id: &str) -> PyResult<Option<PyObject>> {
        self.inner
            .get_marker(marker_id)
            .map(|info| marker_info_to_python(py, info))
            .transpose()
    }
    /// Add a marker at `timeline_start` (timeline seconds) on the stack
    /// (`owner_id` None), on the track with id `owner_id`, or on the clip with
    /// id `owner_id`. Returns the marker id, or None when the owner is not
    /// found (or is a gap) or `id` is already used.
    #[pyo3(signature = (timeline_start, name="".to_string(), comment="".to_string(), color=None, duration=0.0, owner_id=None, created_by=None, id=None))]
    #[allow(clippy::too_many_arguments)]
    fn add_marker(
        &mut self,
        timeline_start: f64,
        name: String,
        comment: String,
        color: Option<String>,
        duration: f64,
        owner_id: Option<String>,
        created_by: Option<String>,
        id: Option<String>,
    ) -> Option<String> {
        let marker = Marker::new(timeline_start, duration, name, comment, color, created_by, id);
        self.inner.add_marker(owner_id.as_deref(), marker)
    }
    /// Change the given fields of a marker (`timeline_start` in timeline
    /// seconds). Returns False when no such marker exists.
    #[pyo3(signature = (marker_id, name=None, comment=None, color=None, timeline_start=None, duration=None))]
    fn update_marker(
        &mut self,
        marker_id: &str,
        name: Option<String>,
        comment: Option<String>,
        color: Option<String>,
        timeline_start: Option<f64>,
        duration: Option<f64>,
    ) -> bool {
        self.inner.update_marker(
            marker_id,
            MarkerUpdate { name, comment, color, timeline_start, duration },
        )
    }
    /// Remove a marker. Returns False when no such marker exists.
    fn delete_marker(&mut self, marker_id: &str) -> bool {
        self.inner.delete_marker(marker_id).is_some()
    }
    /// The stylesheet the player injects for this timeline's text clips: one
    /// `.name { declarations }` rule per declared style.
    fn text_styles_css(&self) -> String {
        self.inner.text_styles_css()
    }
    /// The styles that apply to the clips of the track `track_id`: the
    /// timeline's styles with the track's merged over them by class name.
    /// None when no track has this id.
    fn get_track_text_styles(
        &self,
        py: Python<'_>,
        track_id: &str,
    ) -> PyResult<Option<Vec<PyObject>>> {
        self.inner
            .get_track_text_styles(track_id)
            .map(|styles| text_styles_to_python(py, styles))
            .transpose()
    }
    /// Declare (or redefine) the class `name` on the track `track_id`,
    /// overriding a timeline class of the same name for that track. Returns
    /// False, changing nothing, when no track has this id or the style is
    /// unusable.
    fn set_track_text_style(&mut self, track_id: &str, name: String, declarations: String) -> bool {
        self.inner.set_track_text_style(track_id, name, declarations)
    }
    /// Remove the class `name` from the track `track_id`, returning whether it
    /// was present there. A timeline class of the same name applies again.
    fn remove_track_text_style(&mut self, track_id: &str, name: &str) -> bool {
        self.inner.remove_track_text_style(track_id, name)
    }
    /// The stylesheet the player injects for the clips of the track
    /// `track_id` (merged timeline and track styles), or None.
    fn track_text_styles_css(&self, track_id: &str) -> Option<String> {
        self.inner.track_text_styles_css(track_id)
    }
    /// The tellers effects (filters, Transform, Mask, Volume, Text effects, ...) of the timeline, applied over the whole composited image,
    /// in order, as `{"effect_name", "name", "enabled", "parameters"}` dicts.
    fn get_tellers_effects(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        tellers_effects_to_python(py, self.inner.get_tellers_effects())
    }
    /// Replace every tellers effect of the timeline, applied over the whole composited image with `effects` (dicts like
    /// `get_tellers_effects` returns); other effects stay. False, changing
    /// nothing, when an `effect_name` is unknown.
    fn set_tellers_effects(&mut self, effects: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.set_tellers_effects(tellers_effects_from_python(effects)?))
    }
    /// Append a tellers effect to the timeline, applied over the whole composited image; False when its `effect_name` is unknown.
    fn push_tellers_effect(&mut self, effect: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.push_tellers_effect(tellers_effect_from_python(effect)?))
    }
    /// Insert a tellers effect into the timeline, applied over the whole composited image at `index` (counting tellers effects only).
    fn insert_tellers_effect_at(&mut self, index: usize, effect: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.insert_tellers_effect_at(index, tellers_effect_from_python(effect)?))
    }
    /// Replace the tellers effect of the timeline, applied over the whole composited image at `index`, keeping its other keys;
    /// returns the previous one, or None when out of range.
    fn replace_tellers_effect_at(&mut self, py: Python<'_>, index: usize, effect: &Bound<PyAny>) -> PyResult<Option<PyObject>> {
        let effect = tellers_effect_from_python(effect)?;
        self.inner
            .replace_tellers_effect_at(index, effect)
            .map(|e| tellers_effect_to_python(py, &e))
            .transpose()
    }
    /// Remove and return the tellers effect of the timeline, applied over the whole composited image at `index`.
    fn remove_tellers_effect_at(&mut self, py: Python<'_>, index: usize) -> PyResult<Option<PyObject>> {
        self.inner
            .remove_tellers_effect_at(index)
            .map(|e| tellers_effect_to_python(py, &e))
            .transpose()
    }
    /// Remove every tellers effect of the timeline, applied over the whole composited image, returning whether any was present.
    fn clear_tellers_effects(&mut self) -> bool {
        self.inner.clear_tellers_effects()
    }
    /// Turn the tellers effect of the timeline, applied over the whole composited image at `index` on or off.
    fn set_tellers_effect_enabled(&mut self, index: usize, enabled: bool) -> bool {
        self.inner.set_tellers_effect_enabled(index, enabled)
    }
    /// Set the parameter `key` of the tellers effect of the timeline, applied over the whole composited image at `index`
    /// to `value` (a number, `{"keyframes": [...]}`, a string or a bool);
    /// None removes it.
    #[pyo3(signature = (index, key, value=None))]
    fn set_tellers_effect_parameter(&mut self, index: usize, key: &str, value: Option<&Bound<PyAny>>) -> PyResult<bool> {
        let value = value.filter(|v| !v.is_none()).map(json_from_python).transpose()?;
        Ok(self.inner.set_tellers_effect_parameter(index, key, value))
    }
    /// Every clip group (`metadata["tellers.ai"].groups`), by id, as
    /// `{"id", "parameters"}` dicts.
    fn get_clip_groups(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        self.inner.get_clip_groups().iter().map(|g| clip_group_to_python(py, g)).collect()
    }
    fn get_clip_group(&self, py: Python<'_>, id: &str) -> PyResult<Option<PyObject>> {
        self.inner.get_clip_group(id).map(|g| clip_group_to_python(py, &g)).transpose()
    }
    /// Add or replace a group's transform from a `{"id", "parameters"}` dict;
    /// raises ValueError for an unknown parameter or a bad value.
    fn set_clip_group(&mut self, group: &Bound<PyAny>) -> PyResult<()> {
        let group = clip_group_from_python(group)?;
        self.inner.set_clip_group(&group).map_err(value_error)
    }
    /// Group the clips and composite clips `item_ids` with the transform
    /// `parameters`; returns the new group id. Raises ValueError, changing
    /// nothing, for an id that is not a clip or a bad parameter.
    #[pyo3(signature = (item_ids, parameters=None))]
    fn create_clip_group(&mut self, item_ids: Vec<String>, parameters: Option<&Bound<PyAny>>) -> PyResult<String> {
        let parameters = match parameters {
            Some(p) => generator_parameters_from_python(p)?,
            None => serde_json::Map::new(),
        };
        self.inner.create_clip_group(&item_ids, parameters).map_err(value_error)
    }
    /// Remove a group and take its members out of it; False when it did not exist.
    fn remove_clip_group(&mut self, id: &str) -> bool {
        self.inner.remove_clip_group(id)
    }
    fn get_item_clip_group_id(&self, item_id: &str) -> Option<String> {
        self.inner.get_item_clip_group_id(item_id)
    }
    /// Put a clip or composite clip in a group (None takes it out); False when no such item.
    #[pyo3(signature = (item_id, group_id=None))]
    fn set_item_clip_group_id(&mut self, item_id: &str, group_id: Option<String>) -> bool {
        self.inner.set_item_clip_group_id(item_id, group_id.as_deref())
    }
    fn clip_group_member_ids(&self, id: &str) -> Vec<String> {
        self.inner.clip_group_member_ids(id)
    }
    /// Timeline seconds of the group's keyframe time 0 (its earliest member's start).
    fn clip_group_start(&self, id: &str) -> Option<f64> {
        self.inner.clip_group_start(id)
    }
    /// The group transform at `t` seconds from `clip_group_start`, as the player evaluates it.
    fn clip_group_transform_at(&self, py: Python<'_>, id: &str, t: f64) -> PyResult<Option<PyObject>> {
        self.inner
            .get_clip_group(id)
            .map(|group| {
                let at = group.transform_at(t);
                let value = serde_json::json!({
                    "x": at.x, "y": at.y, "scale_x": at.scale_x, "scale_y": at.scale_y,
                    "rotation": at.rotation, "opacity": at.opacity,
                    "pivot_x": at.pivot_x, "pivot_y": at.pivot_y,
                });
                generator_parameters_to_python(py, &value)
            })
            .transpose()
    }
    /// The tellers effects (filters, Transform, Mask, Volume, Text effects, ...) of the clip or composite clip `item_id`,
    /// in order, as `{"effect_name", "name", "enabled", "parameters"}` dicts.
    fn get_item_tellers_effects(&self, py: Python<'_>, item_id: &str, ) -> PyResult<Option<Vec<PyObject>>> {
        self.inner.get_item_tellers_effects(item_id).map(|e| tellers_effects_to_python(py, e)).transpose()
    }
    /// Replace every tellers effect of the clip or composite clip `item_id` with `effects` (dicts like
    /// `get_item_tellers_effects` returns); other effects stay. False, changing
    /// nothing, when an `effect_name` is unknown, an effect does not go on the kind of track holding the item (no Volume or Pan on a video clip, no picture or text effect on an audio clip, no text effect on anything but a Text clip), or no clip or composite clip has this id.
    fn set_item_tellers_effects(&mut self, item_id: &str, effects: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.set_item_tellers_effects(item_id, tellers_effects_from_python(effects)?))
    }
    /// Append a tellers effect to the clip or composite clip `item_id`; False when its `effect_name` is unknown or it does not go on the kind of track holding the item.
    fn push_item_tellers_effect(&mut self, item_id: &str, effect: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.push_item_tellers_effect(item_id, tellers_effect_from_python(effect)?))
    }
    /// Insert a tellers effect into the clip or composite clip `item_id` at `index` (counting tellers effects only).
    fn insert_item_tellers_effect_at(&mut self, item_id: &str, index: usize, effect: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.insert_item_tellers_effect_at(item_id, index, tellers_effect_from_python(effect)?))
    }
    /// Replace the tellers effect of the clip or composite clip `item_id` at `index`, keeping its other keys;
    /// returns the previous one, or None when out of range.
    fn replace_item_tellers_effect_at(&mut self, py: Python<'_>, item_id: &str, index: usize, effect: &Bound<PyAny>) -> PyResult<Option<PyObject>> {
        let effect = tellers_effect_from_python(effect)?;
        self.inner
            .replace_item_tellers_effect_at(item_id, index, effect)
            .map(|e| tellers_effect_to_python(py, &e))
            .transpose()
    }
    /// Remove and return the tellers effect of the clip or composite clip `item_id` at `index`.
    fn remove_item_tellers_effect_at(&mut self, py: Python<'_>, item_id: &str, index: usize) -> PyResult<Option<PyObject>> {
        self.inner
            .remove_item_tellers_effect_at(item_id, index)
            .map(|e| tellers_effect_to_python(py, &e))
            .transpose()
    }
    /// Remove every tellers effect of the clip or composite clip `item_id`, returning whether any was present.
    fn clear_item_tellers_effects(&mut self, item_id: &str, ) -> bool {
        self.inner.clear_item_tellers_effects(item_id)
    }
    /// Turn the tellers effect of the clip or composite clip `item_id` at `index` on or off.
    fn set_item_tellers_effect_enabled(&mut self, item_id: &str, index: usize, enabled: bool) -> bool {
        self.inner.set_item_tellers_effect_enabled(item_id, index, enabled)
    }
    /// Set the parameter `key` of the tellers effect of the clip or composite clip `item_id` at `index`
    /// to `value` (a number, `{"keyframes": [...]}`, a string or a bool);
    /// None removes it.
    #[pyo3(signature = (item_id, index, key, value=None))]
    fn set_item_tellers_effect_parameter(&mut self, item_id: &str, index: usize, key: &str, value: Option<&Bound<PyAny>>) -> PyResult<bool> {
        let value = value.filter(|v| !v.is_none()).map(json_from_python).transpose()?;
        Ok(self.inner.set_item_tellers_effect_parameter(item_id, index, key, value))
    }
    /// The follow (auto-tracking zoom recipe) of the clip or composite clip `item_id`: the `follow`
    /// parameter of its first enabled Transform, as a `{"region", "target"?, "fill"?, ...}` dict
    /// with the player's option names; None when it has none.
    fn get_item_follow(&self, py: Python<'_>, item_id: &str) -> PyResult<Option<PyObject>> {
        self.inner
            .get_item_follow(item_id)
            .map(|follow| match follow.to_value() {
                serde_json::Value::Object(map) => json_to_python(py, &map),
                _ => unreachable!("a follow serializes to an object"),
            })
            .transpose()
    }
    /// Make the clip or composite clip `item_id` follow `follow["region"]`: write `follow` and the
    /// keyframes `followTransform` computed from it (`transform`: any of `x`, `y`, `scale`) on its
    /// first enabled Transform, keeping its other parameters, or on a new Transform named "Follow".
    /// False, changing nothing, when `follow` or `transform` is invalid, the item is on an audio
    /// track, or no clip or composite clip has this id.
    #[pyo3(signature = (item_id, follow, transform=None))]
    fn set_item_follow(&mut self, item_id: &str, follow: &Bound<PyAny>, transform: Option<&Bound<PyAny>>) -> PyResult<bool> {
        let Some(follow) = tellers_timeline_core::Follow::from_value(&json_from_python(follow)?) else {
            return Ok(false);
        };
        let transform = match transform.filter(|t| !t.is_none()).map(json_from_python).transpose()? {
            None => serde_json::Map::new(),
            Some(serde_json::Value::Object(map)) => map,
            Some(_) => return Ok(false),
        };
        Ok(self.inner.set_item_follow(item_id, &follow, transform))
    }
    /// Drop the follow of the clip or composite clip `item_id`, keeping its baked keyframes as a
    /// plain Transform; True when it had one.
    fn clear_item_follow(&mut self, item_id: &str) -> bool {
        self.inner.clear_item_follow(item_id)
    }
    /// The tellers effects (filters, Transform, Mask, Volume, Text effects, ...) of the track `track_id`,
    /// in order, as `{"effect_name", "name", "enabled", "parameters"}` dicts.
    fn get_track_tellers_effects(&self, py: Python<'_>, track_id: &str, ) -> PyResult<Option<Vec<PyObject>>> {
        self.inner.get_track_tellers_effects(track_id).map(|e| tellers_effects_to_python(py, e)).transpose()
    }
    /// Replace every tellers effect of the track `track_id` with `effects` (dicts like
    /// `get_track_tellers_effects` returns); other effects stay. False, changing
    /// nothing, when an `effect_name` is unknown, an effect does not go on this kind of track (no Volume or Pan on a video track, no picture or text effect on an audio one), or the id is unknown.
    fn set_track_tellers_effects(&mut self, track_id: &str, effects: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.set_track_tellers_effects(track_id, tellers_effects_from_python(effects)?))
    }
    /// Append a tellers effect to the track `track_id`; False when its `effect_name` is unknown or it does not go on this kind of track.
    fn push_track_tellers_effect(&mut self, track_id: &str, effect: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.push_track_tellers_effect(track_id, tellers_effect_from_python(effect)?))
    }
    /// Insert a tellers effect into the track `track_id` at `index` (counting tellers effects only).
    fn insert_track_tellers_effect_at(&mut self, track_id: &str, index: usize, effect: &Bound<PyAny>) -> PyResult<bool> {
        Ok(self.inner.insert_track_tellers_effect_at(track_id, index, tellers_effect_from_python(effect)?))
    }
    /// Replace the tellers effect of the track `track_id` at `index`, keeping its other keys;
    /// returns the previous one, or None when out of range.
    fn replace_track_tellers_effect_at(&mut self, py: Python<'_>, track_id: &str, index: usize, effect: &Bound<PyAny>) -> PyResult<Option<PyObject>> {
        let effect = tellers_effect_from_python(effect)?;
        self.inner
            .replace_track_tellers_effect_at(track_id, index, effect)
            .map(|e| tellers_effect_to_python(py, &e))
            .transpose()
    }
    /// Remove and return the tellers effect of the track `track_id` at `index`.
    fn remove_track_tellers_effect_at(&mut self, py: Python<'_>, track_id: &str, index: usize) -> PyResult<Option<PyObject>> {
        self.inner
            .remove_track_tellers_effect_at(track_id, index)
            .map(|e| tellers_effect_to_python(py, &e))
            .transpose()
    }
    /// Remove every tellers effect of the track `track_id`, returning whether any was present.
    fn clear_track_tellers_effects(&mut self, track_id: &str, ) -> bool {
        self.inner.clear_track_tellers_effects(track_id)
    }
    /// Turn the tellers effect of the track `track_id` at `index` on or off.
    fn set_track_tellers_effect_enabled(&mut self, track_id: &str, index: usize, enabled: bool) -> bool {
        self.inner.set_track_tellers_effect_enabled(track_id, index, enabled)
    }
    /// Set the parameter `key` of the tellers effect of the track `track_id` at `index`
    /// to `value` (a number, `{"keyframes": [...]}`, a string or a bool);
    /// None removes it.
    #[pyo3(signature = (track_id, index, key, value=None))]
    fn set_track_tellers_effect_parameter(&mut self, track_id: &str, index: usize, key: &str, value: Option<&Bound<PyAny>>) -> PyResult<bool> {
        let value = value.filter(|v| !v.is_none()).map(json_from_python).transpose()?;
        Ok(self.inner.set_track_tellers_effect_parameter(track_id, index, key, value))
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

fn json_module(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
    PyModule::import_bound(py, "json")
}

/// A JSON-serializable Python value as JSON.
fn json_from_python(value: &Bound<PyAny>) -> PyResult<serde_json::Value> {
    let text: String = json_module(value.py())?.call_method1("dumps", (value,))?.extract()?;
    serde_json::from_str(&text)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
}

fn json_to_python(py: Python<'_>, value: &serde_json::Map<String, serde_json::Value>) -> PyResult<PyObject> {
    let text = serde_json::to_string(value)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
    Ok(json_module(py)?.call_method1("loads", (text,))?.unbind())
}

fn tellers_effect_to_python(py: Python<'_>, effect: &TellersEffect) -> PyResult<PyObject> {
    let dict = PyDict::new_bound(py);
    dict.set_item("effect_name", &effect.effect_name)?;
    dict.set_item("name", &effect.name)?;
    dict.set_item("enabled", effect.enabled)?;
    dict.set_item("parameters", json_to_python(py, &effect.parameters)?)?;
    Ok(dict.into_py(py))
}

fn tellers_effects_to_python(py: Python<'_>, effects: Vec<TellersEffect>) -> PyResult<Vec<PyObject>> {
    effects.iter().map(|e| tellers_effect_to_python(py, e)).collect()
}

/// A `{"effect_name", "name"?, "enabled"?, "parameters"?}` dict as a tellers effect.
fn tellers_effect_from_python(effect: &Bound<PyAny>) -> PyResult<TellersEffect> {
    if effect.downcast::<PyDict>().is_err() {
        return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "tellers effects must be {\"effect_name\", \"name\", \"enabled\", \"parameters\"} dicts",
        ));
    }
    let mut value = json_from_python(effect)?;
    // `None` for `enabled` / `parameters` means the default.
    if let Some(object) = value.as_object_mut() {
        object.retain(|key, v| !(v.is_null() && (key == "enabled" || key == "parameters")));
    }
    serde_json::from_value(value)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("invalid tellers effect: {e}")))
}

fn tellers_effects_from_python(effects: &Bound<PyAny>) -> PyResult<Vec<TellersEffect>> {
    effects
        .iter()?
        .map(|effect| tellers_effect_from_python(&effect?))
        .collect()
}

/// The tellers effects the player renders, as `{"effect_name", "target",
/// "parameters": [{"name", "kind", "options"?}]}` dicts.
#[pyfunction]
fn tellers_effect_specs(py: Python<'_>) -> PyResult<Vec<PyObject>> {
    use tellers_timeline_core::{EffectTarget, ParameterKind, EFFECT_SPECS};
    EFFECT_SPECS
        .iter()
        .map(|spec| {
            let dict = PyDict::new_bound(py);
            dict.set_item("effect_name", spec.name)?;
            let target = match spec.target {
                EffectTarget::Video => "video",
                EffectTarget::Audio => "audio",
                EffectTarget::Text => "text",
                EffectTarget::Time => "time",
            };
            dict.set_item("target", target)?;
            let parameters = spec
                .parameters
                .iter()
                .map(|param| {
                    let p = PyDict::new_bound(py);
                    p.set_item("name", param.name)?;
                    let kind = match param.kind {
                        ParameterKind::Animatable => "animatable",
                        ParameterKind::Color => "color",
                        ParameterKind::String => "string",
                        ParameterKind::Choice(options) => {
                            p.set_item("options", options.to_vec())?;
                            "choice"
                        }
                        ParameterKind::Bool => "bool",
                        ParameterKind::Points => "points",
                        ParameterKind::Regions => "regions",
                        ParameterKind::Follow => "follow",
                    };
                    p.set_item("kind", kind)?;
                    Ok(p.into_py(py))
                })
                .collect::<PyResult<Vec<PyObject>>>()?;
            dict.set_item("parameters", parameters)?;
            Ok(dict.into_py(py))
        })
        .collect()
}

/// Why `effect` (a tellers effect dict) does not match the player's specs,
/// or None when it does.
#[pyfunction]
fn validate_tellers_effect(effect: &Bound<PyAny>) -> PyResult<Option<String>> {
    Ok(tellers_effect_from_python(effect)?.validate().err())
}

/// The value at `t` seconds from the clip start of an animatable parameter
/// (a number or `{"keyframes": [...]}`), as the player evaluates it; None
/// when it is not usable.
#[pyfunction]
fn evaluate_animatable(value: &Bound<PyAny>, t: f64) -> PyResult<Option<f64>> {
    Ok(Animatable::from_value(&json_from_python(value)?).map(|a| a.value_at(t)))
}

#[pymodule]
fn tellers_timeline(_py: Python, m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<PyMediaReference>()?;
    m.add_class::<PyMediaReferencePosition>()?;
    m.add_class::<PyMediaReferenceCrop>()?;
    m.add_class::<PyEffect>()?;
    m.add_class::<PyClip>()?;
    m.add_class::<PyGap>()?;
    m.add_class::<PyTransition>()?;
    m.add_class::<PyItem>()?;
    m.add_class::<PyTrack>()?;
    m.add_class::<PyStack>()?;
    m.add_class::<PyTimeline>()?;
    m.add_function(wrap_pyfunction!(tellers_effect_specs, m)?)?;
    m.add_function(wrap_pyfunction!(validate_tellers_effect, m)?)?;
    m.add_function(wrap_pyfunction!(evaluate_animatable, m)?)?;
    m.add("MARKER_COLORS", tellers_timeline_core::MARKER_COLORS.to_vec())?;
    m.add(
        "AUDIO_TRANSITION_TYPES",
        AudioTransitionKind::ALL.map(AudioTransitionKind::transition_type).to_vec(),
    )?;
    Ok(())
}
