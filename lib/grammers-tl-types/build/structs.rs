//! Code to generate Rust's `struct`'s from TL definitions.

use crate::grouper;
use crate::rustifier::{rusty_attr_name, rusty_class_name, rusty_type_name, rusty_type_path};
use grammers_tl_parser::{Category, Definition, ParameterType};
use std::io::{self, Write};

/// Defines the `struct` corresponding to the definition:
///
/// ```
/// pub struct Name {
///     pub field: Type,
/// }
/// ```
fn write_struct<W: Write>(file: &mut W, indent: &str, def: &Definition) -> io::Result<()> {
    // Define struct
    writeln!(
        file,
        "{}pub struct {} {{",
        indent,
        rusty_class_name(&def.name)
    )?;
    for param in def.params.iter() {
        match param.ty {
            ParameterType::Flags => {
                // Flags are computed on-the-fly, not stored
            }
            ParameterType::Normal { .. } => {
                writeln!(
                    file,
                    "{}    pub {}: {},",
                    indent,
                    rusty_attr_name(param),
                    rusty_type_name(param)
                )?;
            }
        }
    }
    writeln!(file, "{}}}", indent)?;
    Ok(())
}

/// Defines the `impl Identifiable` corresponding to the definition:
///
/// ```
/// impl crate::Identifiable for Name {
///     fn constructor_id() -> u32 { 123 }
/// }
/// ```
fn write_identifiable<W: Write>(file: &mut W, indent: &str, def: &Definition) -> io::Result<()> {
    writeln!(
        file,
        "{}impl crate::Identifiable for {} {{",
        indent,
        rusty_class_name(&def.name)
    )?;
    writeln!(
        file,
        "{}    const CONSTRUCTOR_ID: u32 = {};",
        indent,
        def.id.unwrap()
    )?;
    writeln!(file, "{}}}", indent)?;
    Ok(())
}

/// Defines the `impl Serializable` corresponding to the definition:
///
/// ```
/// impl crate::Serializable for Name {
///     fn serialize<B: std::io::Write>(&self, buf: &mut B) -> std::io::Result<()> {
///         self.field.serialize(buf)?;
///         Ok(())
///     }
/// }
/// ```
fn write_serializable<W: Write>(file: &mut W, indent: &str, def: &Definition) -> io::Result<()> {
    writeln!(
        file,
        "{}impl crate::Serializable for {} {{",
        indent,
        rusty_class_name(&def.name)
    )?;
    writeln!(
        file,
        "{}    fn serialize<B: std::io::Write>(&self, {}buf: &mut B) -> std::io::Result<()> {{",
        indent,
        if def.params.is_empty() { "_" } else { "" }
    )?;

    for param in def.params.iter() {
        write!(file, "{}        ", indent)?;
        match &param.ty {
            ParameterType::Flags => {
                write!(file, "buf.write(&(0u32")?;

                // Compute flags as a single expression
                for p in def.params.iter() {
                    match &p.ty {
                        ParameterType::Normal {
                            ty,
                            flag: Some(flag),
                        } if flag.name == param.name => {
                            // We make sure this `p` uses the flag we're currently
                            // parsing by comparing (`p`'s) `flag.name == param.name`.

                            // OR (if the flag is present) the correct bit index.
                            // Only the special-cased "true" flags are booleans.
                            write!(
                                file,
                                " | if self.{}{} {{ {} }} else {{ 0 }}",
                                rusty_attr_name(p),
                                if ty.name == "true" { "" } else { ".is_some()" },
                                1 << flag.index
                            )?;
                        }
                        _ => {}
                    }
                }

                writeln!(file, ").to_le_bytes())?;")?;
            }
            ParameterType::Normal { ty, flag } => {
                // The `true` type is not serialized
                if ty.name != "true" {
                    if flag.is_some() {
                        writeln!(
                            file,
                            "if let Some(ref x) = self.{} {{ ",
                            rusty_attr_name(param)
                        )?;
                        writeln!(file, "{}            x.serialize(buf)?;", indent)?;
                        writeln!(file, "{}        }}", indent)?;
                    } else {
                        writeln!(file, "self.{}.serialize(buf)?;", rusty_attr_name(param))?;
                    }
                }
            }
        }
    }

    writeln!(file, "{}        Ok(())", indent)?;
    writeln!(file, "{}    }}", indent)?;
    writeln!(file, "{}}}", indent)?;
    Ok(())
}

/// Defines the `impl Deserializable` corresponding to the definition:
///
/// ```
/// impl crate::Deserializable for Name {
///     fn deserialize<B: std::io::Read>(buf: &mut B) -> std::io::Result<Self> {
///         let field = FieldType::deserialize(buf)?;
///         Ok(Name { field })
///     }
/// }
/// ```
fn write_deserializable<W: Write>(file: &mut W, indent: &str, def: &Definition) -> io::Result<()> {
    writeln!(
        file,
        "{}impl crate::Deserializable for {} {{",
        indent,
        rusty_class_name(&def.name)
    )?;
    writeln!(
        file,
        "{}    fn deserialize<B: std::io::Read>({}buf: &mut B) -> std::io::Result<Self> {{",
        indent,
        if def.params.is_empty() { "_" } else { "" }
    )?;

    for param in def.params.iter() {
        write!(file, "{}        ", indent)?;
        match &param.ty {
            ParameterType::Flags => {
                writeln!(
                    file,
                    "let {} = u32::deserialize(buf)?;",
                    rusty_attr_name(param)
                )?;
            }
            ParameterType::Normal { ty, flag } => {
                if ty.name == "true" {
                    let flag = flag
                        .as_ref()
                        .expect("the `true` type must always be used in a flag");
                    writeln!(
                        file,
                        "let {} = ({} & {}) != 0;",
                        rusty_attr_name(param),
                        flag.name,
                        1 << flag.index
                    )?;
                } else {
                    write!(file, "let {} = ", rusty_attr_name(param))?;
                    if let Some(ref flag) = flag {
                        writeln!(file, "if ({} & {}) != 0 {{", flag.name, 1 << flag.index)?;
                        write!(file, "{}            Some(", indent)?;
                    }
                    write!(file, "{}::deserialize(buf)?", rusty_type_path(param))?;
                    if flag.is_some() {
                        writeln!(file, ")")?;
                        writeln!(file, "{}        }} else {{", indent)?;
                        writeln!(file, "{}            None", indent)?;
                        write!(file, "{}        }}", indent)?;
                    }
                    writeln!(file, ";")?;
                }
            }
        }
    }

    writeln!(
        file,
        "{}        Ok({} {{",
        indent,
        rusty_class_name(&def.name)
    )?;

    for param in def.params.iter() {
        write!(file, "{}            ", indent)?;
        match &param.ty {
            ParameterType::Flags => {}
            ParameterType::Normal { .. } => {
                writeln!(file, "{},", rusty_attr_name(param))?;
            }
        }
    }
    writeln!(file, "{}        }})", indent)?;
    writeln!(file, "{}    }}", indent)?;
    writeln!(file, "{}}}", indent)?;
    Ok(())
}

/// Writes an entire definition as Rust code (`struct` and `impl`).
fn write_definition<W: Write>(file: &mut W, indent: &str, def: &Definition) -> io::Result<()> {
    write_struct(file, indent, def)?;
    write_identifiable(file, indent, def)?;
    write_serializable(file, indent, def)?;
    write_deserializable(file, indent, def)?;
    Ok(())
}

/// Write an entire module for the desired category.
pub(crate) fn write_category_mod<W: Write>(
    mut file: &mut W,
    category: Category,
    definitions: &Vec<Definition>,
) -> io::Result<()> {
    // Begin outermost mod
    writeln!(
        file,
        "pub mod {} {{",
        match category {
            Category::Types => "types",
            Category::Functions => "functions",
        }
    )?;

    let grouped = grouper::group_by_ns(definitions, category);
    let mut sorted_keys: Vec<&String> = grouped.keys().collect();
    sorted_keys.sort();
    for key in sorted_keys.into_iter() {
        // Begin possibly inner mod
        let indent = if key.is_empty() {
            "    "
        } else {
            writeln!(file, "    pub mod {} {{", key)?;
            "        "
        };

        for definition in grouped[key].iter() {
            write_definition(&mut file, indent, definition)?;
        }

        // End possibly inner mod
        if !key.is_empty() {
            writeln!(file, "    }}")?;
        }
    }

    // End outermost mod
    writeln!(file, "}}")
}
