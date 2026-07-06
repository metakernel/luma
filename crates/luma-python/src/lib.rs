#![allow(unsafe_op_in_unsafe_fn)]
#![allow(clippy::useless_conversion)]

use pyo3::IntoPyObjectExt;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};

use luma::lumba;
use luma::parser::FileId;
use luma::syntax::{
    LumaKey, LumaMapping, LumaMappingEntry, LumaNumber, LumaSequence, LumaTag, LumaTagName,
    LumaTaggedValue,
};
use luma::{LumaNull, LumaValue, SyntaxIndex, SyntaxNodeId};

#[pyclass(module = "luma_python", skip_from_py_object)]
#[derive(Debug, Default, Clone, Copy)]
struct Parser;

#[pymethods]
impl Parser {
    #[new]
    fn new() -> Self {
        Self
    }

    fn parse_str(
        &self,
        py: Python<'_>,
        file_id: u32,
        name: &str,
        text: &str,
    ) -> PyResult<Py<PyDict>> {
        parse_str(py, file_id, name, text)
    }

    fn format_str(
        &self,
        py: Python<'_>,
        file_id: u32,
        name: &str,
        text: &str,
    ) -> PyResult<Py<PyDict>> {
        format_str(py, file_id, name, text)
    }
}

#[pyfunction]
fn version() -> &'static str {
    luma::version()
}

#[pyfunction]
fn parse_str(py: Python<'_>, file_id: u32, name: &str, text: &str) -> PyResult<Py<PyDict>> {
    let parsed = luma::Parser::new().parse_str(FileId(file_id), name, text);
    let out = PyDict::new(py);
    out.set_item("source", parsed.source.as_str())?;
    out.set_item("document_count", parsed.file.documents.len())?;
    out.set_item("diagnostics", diagnostics_to_py(py, &parsed.diagnostics)?)?;
    out.set_item(
        "syntax_index",
        syntax_index_to_py(py, &parsed.syntax_index())?,
    )?;
    Ok(out.unbind())
}

#[pyfunction]
fn format_str(py: Python<'_>, file_id: u32, name: &str, text: &str) -> PyResult<Py<PyDict>> {
    let formatted = luma::parser::format_str(FileId(file_id), name, text);
    let out = PyDict::new(py);
    out.set_item("text", formatted.formatted.text)?;
    out.set_item("changed", formatted.formatted.changed)?;
    out.set_item(
        "diagnostics",
        diagnostics_to_py(py, &formatted.parsed.diagnostics)?,
    )?;
    Ok(out.unbind())
}

#[pyfunction]
fn to_lumba_value_image(py: Python<'_>, values: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    let values = py_to_luma_values(values)?;
    let bytes = lumba::try_to_lumba_value_image(&values).map_err(lumba_error)?;
    Ok(PyBytes::new(py, &bytes).unbind())
}

#[pyfunction]
fn from_lumba_value_image(py: Python<'_>, bytes: &[u8]) -> PyResult<Py<PyAny>> {
    let values = lumba::try_from_lumba_value_image(bytes).map_err(lumba_error)?;
    let out = PyList::empty(py);
    for value in &values {
        out.append(luma_value_to_py(py, value)?)?;
    }
    Ok(out.into_any().unbind())
}

#[pyfunction]
fn read_lumba_value_image(py: Python<'_>, bytes: &[u8]) -> PyResult<Py<PyAny>> {
    from_lumba_value_image(py, bytes)
}

#[pyfunction]
fn write_lumba_value_image(py: Python<'_>, values: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    to_lumba_value_image(py, values)
}

#[pymodule]
fn luma_python(py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Parser>()?;
    module.add_function(wrap_pyfunction!(version, module)?)?;
    module.add_function(wrap_pyfunction!(parse_str, module)?)?;
    module.add_function(wrap_pyfunction!(format_str, module)?)?;
    module.add_function(wrap_pyfunction!(to_lumba_value_image, module)?)?;
    module.add_function(wrap_pyfunction!(from_lumba_value_image, module)?)?;

    let lumba_module = PyModule::new(py, "lumba")?;
    lumba_module.add_function(wrap_pyfunction!(to_lumba_value_image, &lumba_module)?)?;
    lumba_module.add_function(wrap_pyfunction!(from_lumba_value_image, &lumba_module)?)?;
    lumba_module.add_function(wrap_pyfunction!(read_lumba_value_image, &lumba_module)?)?;
    lumba_module.add_function(wrap_pyfunction!(write_lumba_value_image, &lumba_module)?)?;
    module.add_submodule(&lumba_module)?;
    Ok(())
}

fn diagnostics_to_py(py: Python<'_>, diagnostics: &[luma::Diagnostic]) -> PyResult<Py<PyAny>> {
    let out = PyList::empty(py);
    for diagnostic in diagnostics {
        let item = PyDict::new(py);
        item.set_item("code", diagnostic.code.code())?;
        item.set_item("severity", format!("{:?}", diagnostic.severity))?;
        item.set_item("message", &diagnostic.message)?;
        item.set_item(
            "primary_span",
            option_span_to_py(py, diagnostic.primary_span)?,
        )?;

        let related = PyList::empty(py);
        for related_span in &diagnostic.related_spans {
            let related_item = PyDict::new(py);
            related_item.set_item("span", span_to_py(py, related_span.span)?)?;
            related_item.set_item("message", &related_span.message)?;
            related.append(related_item)?;
        }
        item.set_item("related_spans", related)?;
        item.set_item("notes", &diagnostic.notes)?;
        out.append(item)?;
    }
    Ok(out.into_any().unbind())
}

fn syntax_index_to_py(py: Python<'_>, index: &SyntaxIndex) -> PyResult<Py<PyAny>> {
    let out = PyList::empty(py);
    for root in &index.root_ids {
        push_syntax_node(py, index, *root, &out)?;
    }
    Ok(out.into_any().unbind())
}

fn push_syntax_node(
    py: Python<'_>,
    index: &SyntaxIndex,
    id: SyntaxNodeId,
    out: &Bound<'_, PyList>,
) -> PyResult<()> {
    if let Some(node) = index.node(id) {
        let item = PyDict::new(py);
        item.set_item("id", node.id.0)?;
        item.set_item("kind", format!("{:?}", node.kind))?;
        item.set_item("span", span_to_py(py, node.span)?)?;
        item.set_item("parent", node.parent.map(|parent| parent.0))?;
        let children: Vec<u32> = index.children(id).iter().map(|child| child.0).collect();
        item.set_item("children", children)?;
        out.append(item)?;
        for child in index.children(id) {
            push_syntax_node(py, index, *child, out)?;
        }
    }
    Ok(())
}

fn option_span_to_py(py: Python<'_>, span: Option<luma::syntax::Span>) -> PyResult<Py<PyAny>> {
    match span {
        Some(span) => span_to_py(py, span),
        None => Ok(py.None()),
    }
}

fn span_to_py(py: Python<'_>, span: luma::syntax::Span) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    out.set_item("file_id", span.file_id.0)?;
    out.set_item("start", span.start)?;
    out.set_item("end", span.end)?;
    Ok(out.into_any().unbind())
}

fn py_to_luma_values(values: &Bound<'_, PyAny>) -> PyResult<Vec<LumaValue>> {
    if let Ok(list) = values.cast::<PyList>() {
        list.iter().map(|item| py_to_luma_value(&item)).collect()
    } else if let Ok(tuple) = values.cast::<PyTuple>() {
        tuple.iter().map(|item| py_to_luma_value(&item)).collect()
    } else {
        Ok(vec![py_to_luma_value(values)?])
    }
}

fn py_to_luma_value(obj: &Bound<'_, PyAny>) -> PyResult<LumaValue> {
    if obj.is_none() {
        return Ok(LumaValue::Null(LumaNull));
    }
    if obj.is_instance_of::<PyBool>() {
        return Ok(LumaValue::Boolean(obj.extract()?));
    }
    if obj.is_instance_of::<PyInt>() {
        return Ok(LumaValue::Number(LumaNumber::Integer(obj.extract()?)));
    }
    if obj.is_instance_of::<PyFloat>() {
        let value = obj.extract::<f64>()?;
        if !value.is_finite() {
            return Err(PyValueError::new_err("Luma floats must be finite"));
        }
        return Ok(LumaValue::Number(LumaNumber::Float(value)));
    }
    if obj.is_instance_of::<PyString>() {
        return Ok(LumaValue::String(obj.extract()?));
    }
    if let Ok(list) = obj.cast::<PyList>() {
        let items = list
            .iter()
            .map(|item| py_to_luma_value(&item))
            .collect::<PyResult<_>>()?;
        return Ok(LumaValue::Sequence(LumaSequence { items, span: None }));
    }
    if let Ok(tuple) = obj.cast::<PyTuple>() {
        let items = tuple
            .iter()
            .map(|item| py_to_luma_value(&item))
            .collect::<PyResult<_>>()?;
        return Ok(LumaValue::Sequence(LumaSequence { items, span: None }));
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
        if let (Ok(Some(tag)), Ok(Some(value))) =
            (dict.get_item("__luma_tag__"), dict.get_item("value"))
        {
            let tag = tag.extract::<String>()?;
            let value = py_to_luma_value(&value)?;
            let span = luma::syntax::Span::new(FileId(0), 0, 0);
            return Ok(LumaValue::Tagged(LumaTaggedValue {
                tag: LumaTag {
                    name: LumaTagName { value: tag, span },
                    span,
                },
                value: Box::new(value),
                span: None,
            }));
        }

        let mut entries = Vec::with_capacity(dict.len());
        for (key, value) in dict.iter() {
            entries.push(LumaMappingEntry {
                key: py_to_luma_key(&key)?,
                value: py_to_luma_value(&value)?,
                span: None,
            });
        }
        return Ok(LumaValue::Mapping(LumaMapping {
            entries,
            duplicate_keys: Vec::new(),
            span: None,
        }));
    }
    Err(PyTypeError::new_err(
        "expected None, bool, int, float, str, list, tuple, or dict",
    ))
}

fn py_to_luma_key(obj: &Bound<'_, PyAny>) -> PyResult<LumaKey> {
    if obj.is_instance_of::<PyBool>() {
        Ok(LumaKey::Boolean(obj.extract()?))
    } else if obj.is_instance_of::<PyInt>() {
        Ok(LumaKey::Number(LumaNumber::Integer(obj.extract()?)))
    } else if obj.is_instance_of::<PyFloat>() {
        let value = obj.extract::<f64>()?;
        if !value.is_finite() {
            return Err(PyValueError::new_err("Luma numeric keys must be finite"));
        }
        Ok(LumaKey::Number(LumaNumber::Float(value)))
    } else if obj.is_instance_of::<PyString>() {
        Ok(LumaKey::String(obj.extract()?))
    } else {
        Err(PyTypeError::new_err(
            "Luma mapping keys must be bool, int, float, or str",
        ))
    }
}

fn luma_value_to_py(py: Python<'_>, value: &LumaValue) -> PyResult<Py<PyAny>> {
    match value {
        LumaValue::Null(_) => Ok(py.None()),
        LumaValue::Boolean(value) => value.into_py_any(py),
        LumaValue::Number(LumaNumber::Integer(value)) => value.into_py_any(py),
        LumaValue::Number(LumaNumber::Float(value)) => value.into_py_any(py),
        LumaValue::String(value) => value.into_py_any(py),
        LumaValue::Sequence(sequence) => {
            let out = PyList::empty(py);
            for item in &sequence.items {
                out.append(luma_value_to_py(py, item)?)?;
            }
            Ok(out.into_any().unbind())
        }
        LumaValue::Mapping(mapping) => mapping_to_py(py, mapping),
        LumaValue::Tagged(tagged) => {
            let out = PyDict::new(py);
            out.set_item("__luma_tag__", &tagged.tag.name.value)?;
            out.set_item("value", luma_value_to_py(py, &tagged.value)?)?;
            Ok(out.into_any().unbind())
        }
        LumaValue::Function(value) | LumaValue::UserData(value) | LumaValue::HostObject(value) => {
            let out = PyDict::new(py);
            out.set_item("__luma_host__", &value.kind)?;
            out.set_item("label", &value.label)?;
            Ok(out.into_any().unbind())
        }
    }
}

fn mapping_to_py(py: Python<'_>, mapping: &LumaMapping) -> PyResult<Py<PyAny>> {
    let all_string_keys = mapping
        .entries
        .iter()
        .all(|entry| matches!(entry.key, LumaKey::String(_)));
    if all_string_keys {
        let out = PyDict::new(py);
        for entry in &mapping.entries {
            if let LumaKey::String(key) = &entry.key {
                out.set_item(key, luma_value_to_py(py, &entry.value)?)?;
            }
        }
        Ok(out.into_any().unbind())
    } else {
        let pairs = PyList::empty(py);
        for entry in &mapping.entries {
            let pair = PyTuple::new(
                py,
                [
                    luma_key_to_py(py, &entry.key)?,
                    luma_value_to_py(py, &entry.value)?,
                ],
            )?;
            pairs.append(pair)?;
        }
        let out = PyDict::new(py);
        out.set_item("__luma_mapping__", pairs)?;
        Ok(out.into_any().unbind())
    }
}

fn luma_key_to_py(py: Python<'_>, key: &LumaKey) -> PyResult<Py<PyAny>> {
    match key {
        LumaKey::String(value) => value.into_py_any(py),
        LumaKey::Number(LumaNumber::Integer(value)) => value.into_py_any(py),
        LumaKey::Number(LumaNumber::Float(value)) => value.into_py_any(py),
        LumaKey::Boolean(value) => value.into_py_any(py),
        LumaKey::Host(value) => {
            let out = PyDict::new(py);
            out.set_item("__luma_host_key__", &value.kind)?;
            out.set_item("label", &value.label)?;
            Ok(out.into_any().unbind())
        }
    }
}

fn lumba_error(error: lumba::LumbaError) -> PyErr {
    PyValueError::new_err(format!("{}: {}", error.code().as_str(), error))
}
