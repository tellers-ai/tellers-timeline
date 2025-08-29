use serde::Serialize;
use serde_json::ser::{CompactFormatter, PrettyFormatter, Serializer};
use serde_json::Result as SerdeResult;
use std::io;

fn round_to_precision(value: f64, precision: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let factor = 10f64.powi(precision as i32);
    (value * factor).round() / factor
}

struct PrecisionFormatter<F: serde_json::ser::Formatter> {
    inner: F,
    precision: Option<usize>,
}

impl<F: serde_json::ser::Formatter> PrecisionFormatter<F> {
    fn new(inner: F, precision: Option<usize>) -> Self {
        Self { inner, precision }
    }
}

impl<F: serde_json::ser::Formatter> serde_json::ser::Formatter for PrecisionFormatter<F> {
    fn write_null<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.inner.write_null(writer)
    }
    fn write_bool<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: bool) -> io::Result<()> {
        self.inner.write_bool(writer, value)
    }
    fn write_i8<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: i8) -> io::Result<()> {
        self.inner.write_i8(writer, value)
    }
    fn write_i16<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: i16) -> io::Result<()> {
        self.inner.write_i16(writer, value)
    }
    fn write_i32<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: i32) -> io::Result<()> {
        self.inner.write_i32(writer, value)
    }
    fn write_i64<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: i64) -> io::Result<()> {
        self.inner.write_i64(writer, value)
    }
    fn write_u8<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: u8) -> io::Result<()> {
        self.inner.write_u8(writer, value)
    }
    fn write_u16<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: u16) -> io::Result<()> {
        self.inner.write_u16(writer, value)
    }
    fn write_u32<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: u32) -> io::Result<()> {
        self.inner.write_u32(writer, value)
    }
    fn write_u64<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: u64) -> io::Result<()> {
        self.inner.write_u64(writer, value)
    }
    fn write_f32<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: f32) -> io::Result<()> {
        match self.precision {
            Some(p) => self
                .inner
                .write_f32(writer, round_to_precision(value as f64, p) as f32),
            None => self.inner.write_f32(writer, value),
        }
    }
    fn write_f64<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: f64) -> io::Result<()> {
        match self.precision {
            Some(p) => self.inner.write_f64(writer, round_to_precision(value, p)),
            None => self.inner.write_f64(writer, value),
        }
    }
    fn begin_string<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.inner.begin_string(writer)
    }
    fn end_string<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.inner.end_string(writer)
    }
    fn write_string_fragment<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        self.inner.write_string_fragment(writer, fragment)
    }
    fn write_char_escape<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        char_escape: serde_json::ser::CharEscape,
    ) -> io::Result<()> {
        self.inner.write_char_escape(writer, char_escape)
    }
    fn begin_array<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.inner.begin_array(writer)
    }
    fn end_array<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.inner.end_array(writer)
    }
    fn begin_array_value<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        self.inner.begin_array_value(writer, first)
    }
    fn end_array_value<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.inner.end_array_value(writer)
    }
    fn begin_object<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.inner.begin_object(writer)
    }
    fn end_object<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.inner.end_object(writer)
    }
    fn begin_object_key<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        self.inner.begin_object_key(writer, first)
    }
    fn end_object_key<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.inner.end_object_key(writer)
    }
    fn begin_object_value<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.inner.begin_object_value(writer)
    }
    fn end_object_value<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.inner.end_object_value(writer)
    }
}

pub fn to_json_with_precision<T: Serialize>(
    value: &T,
    precision: Option<usize>,
    pretty: bool,
) -> SerdeResult<String> {
    let mut out = Vec::new();
    if pretty {
        let fmt = PrecisionFormatter::new(PrettyFormatter::with_indent(b"  "), precision);
        let mut ser = Serializer::with_formatter(&mut out, fmt);
        value.serialize(&mut ser)?;
    } else {
        let fmt = PrecisionFormatter::new(CompactFormatter, precision);
        let mut ser = Serializer::with_formatter(&mut out, fmt);
        value.serialize(&mut ser)?;
    }
    Ok(String::from_utf8(out).expect("valid utf8"))
}
