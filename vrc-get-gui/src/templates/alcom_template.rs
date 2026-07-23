#![doc = include_str!("./alcom_template.md")]

use crate::templates::{RESERVED_TEMPLATE_PREFIX, UNNAMED_TEMPLATE_PREFIX, VCC_TEMPLATE_PREFIX};
use indexmap::IndexMap;
use serde::de::{Error, Unexpected};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::Formatter;
use std::path::PathBuf;
use vrc_get_vpm::version::{UnityVersion, VersionRange};

static MAGIC: &str = "com.anatawa12.vrc-get.custom-template";

#[derive(Serialize, Deserialize)]
struct MagicParser {
    #[serde(rename = "$type")]
    ty: String,
    #[serde(rename = "formatVersion")]
    format_version: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlcomTemplateContentV1 {
    pub display_name: String,
    pub update_date: Option<chrono::DateTime<chrono::offset::Utc>>,
    pub id: Option<TemplateId>,
    pub base: TemplateId,
    pub unity_version: Option<VersionRange>,
    #[serde(default)]
    pub vpm_dependencies: IndexMap<String, VersionRange>,
    #[serde(default)]
    pub unity_packages: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlcomTemplateContentV2 {
    pub display_name: String,
    pub update_date: Option<chrono::DateTime<chrono::offset::Utc>>,
    pub id: Option<TemplateId>,
    pub unity_version: Option<VersionRange>,
    #[serde(flatten)]
    pub kind: AlcomTemplateKindContent,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "templateKind", rename_all = "camelCase")]
enum AlcomTemplateKindContent {
    Derived {
        base: TemplateId,
        #[serde(default)]
        vpm_dependencies: IndexMap<String, VersionRange>,
        #[serde(default)]
        unity_packages: Vec<PathBuf>,
    },
    ProjectArchive {
        archive: AlcomTemplateArchive,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlcomTemplateArchive {
    #[serde(rename = "format")]
    pub format: AlcomTemplateArchiveFormat,
    pub unity_version: UnityVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AlcomTemplateArchiveFormat {
    #[serde(rename = "tar.gz")]
    TarGz,
}

struct TemplateId(String);

impl<'de> Deserialize<'de> for TemplateId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = TemplateId;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("a valid alcom template id")
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if v.is_empty()
                    || v.chars()
                        .any(|c| !matches!(c, '0'..='9' | 'A'..='Z' | 'a'..='z' | '.' | '_' | '-'))
                {
                    return Err(E::invalid_value(
                        Unexpected::Str(&v),
                        &"a valid alcom template id",
                    ));
                }

                Ok(TemplateId(v))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if v.is_empty()
                    || v.chars()
                        .any(|c| !matches!(c, '0'..='9' | 'A'..='Z' | 'a'..='z' | '.' | '_' | '-'))
                {
                    return Err(E::invalid_value(
                        Unexpected::Str(v),
                        &"a valid alcom template id",
                    ));
                }

                Ok(TemplateId(v.to_string()))
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

impl Serialize for TemplateId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        String::serialize(&self.0, serializer)
    }
}

#[derive(Serialize)]
struct AlcomTemplateSerializeV1 {
    #[serde(flatten)]
    magic: MagicParser,
    #[serde(flatten)]
    content: AlcomTemplateContentV1,
}

#[derive(Serialize)]
struct AlcomTemplateSerializeV2 {
    #[serde(flatten)]
    magic: MagicParser,
    #[serde(flatten)]
    content: AlcomTemplateContentV2,
}

#[derive(Clone)]
pub struct AlcomTemplate {
    pub display_name: String,
    pub update_date: Option<chrono::DateTime<chrono::offset::Utc>>,
    pub id: Option<String>,
    pub base: Option<String>,
    pub unity_version: Option<VersionRange>,
    pub vpm_dependencies: IndexMap<String, VersionRange>,
    pub unity_packages: Vec<PathBuf>,
    pub archive: Option<AlcomTemplateArchive>,
}

impl AlcomTemplate {
    pub fn is_derived(&self) -> bool {
        self.base.is_some() && self.archive.is_none()
    }

    pub fn is_project_archive(&self) -> bool {
        self.archive.is_some()
    }
}

pub fn parse_alcom_template(alcom_template: &[u8]) -> serde_json::Result<AlcomTemplate> {
    // For future extension, we only parse file heading until first '\0' or null byte.
    // We may extend `.alcomtemplate` file to include binary data, and JSON is very bad at
    // holding binary data, so I make room for non-JSON data at the tail of the file.
    // JSON data has ending character ('}'), so se actually don't need to have special '\0' char,
    // but this is simple and the border of JSON and binary data is clear.

    let json_end = alcom_template
        .iter()
        .position(|&x| x == 0)
        .unwrap_or(alcom_template.len());
    let json = &alcom_template[..json_end];

    // first, parse magic and format version
    let magic: MagicParser = serde_json::from_slice(json)?;
    if magic.ty != MAGIC {
        return Err(serde_json::Error::custom("Invalid $type"));
    }

    let Some((major, _minor)) = parse_format_version(&magic.format_version) else {
        return Err(serde_json::Error::custom(format!(
            "Unsupported formatVersion: {}",
            magic.format_version
        )));
    };

    // we've checked the version is correct! Parse the contents now.
    match major {
        1 => {
            let template = serde_json::from_slice::<AlcomTemplateContentV1>(json)?;
            validate_template_id(&template.id)?;
            validate_base_id(&template.base.0)?;

            Ok(AlcomTemplate {
                display_name: template.display_name,
                update_date: template.update_date,
                id: template.id.map(|id| id.0),
                base: Some(template.base.0),
                unity_version: template.unity_version,
                vpm_dependencies: template.vpm_dependencies,
                unity_packages: template.unity_packages,
                archive: None,
            })
        }
        2 => {
            let template = serde_json::from_slice::<AlcomTemplateContentV2>(json)?;
            validate_template_id(&template.id)?;

            match template.kind {
                AlcomTemplateKindContent::Derived {
                    base,
                    vpm_dependencies,
                    unity_packages,
                } => {
                    validate_base_id(&base.0)?;

                    Ok(AlcomTemplate {
                        display_name: template.display_name,
                        update_date: template.update_date,
                        id: template.id.map(|id| id.0),
                        base: Some(base.0),
                        unity_version: template.unity_version,
                        vpm_dependencies,
                        unity_packages,
                        archive: None,
                    })
                }
                AlcomTemplateKindContent::ProjectArchive { archive } => {
                    if alcom_template_project_archive_payload(alcom_template).is_none() {
                        return Err(serde_json::Error::custom(
                            "Project archive template has no archive payload",
                        ));
                    }

                    Ok(AlcomTemplate {
                        display_name: template.display_name,
                        update_date: template.update_date,
                        id: template.id.map(|id| id.0),
                        base: None,
                        unity_version: template.unity_version,
                        vpm_dependencies: IndexMap::new(),
                        unity_packages: Vec::new(),
                        archive: Some(archive),
                    })
                }
            }
        }
        _ => Err(serde_json::Error::custom(format!(
            "Unsupported formatVersion: {}",
            magic.format_version
        ))),
    }
}

fn validate_template_id(id: &Option<TemplateId>) -> serde_json::Result<()> {
    if let Some(id) = id
        && !is_valid_id(&id.0)
    {
        return Err(serde_json::Error::invalid_value(
            Unexpected::Str(&id.0),
            &"a valid alcom template id",
        ));
    }

    Ok(())
}

fn validate_base_id(id: &str) -> serde_json::Result<()> {
    if !is_valid_base_id(id) {
        return Err(serde_json::Error::invalid_value(
            Unexpected::Str(id),
            &"a valid alcom template id",
        ));
    }

    Ok(())
}

fn is_valid_id(id: &str) -> bool {
    if id.starts_with(RESERVED_TEMPLATE_PREFIX) {
        if let Some(uuid) = id.strip_prefix(UNNAMED_TEMPLATE_PREFIX) {
            // 32 of lowercase hex char
            uuid.len() == 32
                && (uuid.as_bytes().iter()).all(|&b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        } else {
            // reserved id
            false
        }
    } else {
        true
    }
}

fn is_valid_base_id(id: &str) -> bool {
    !(id.starts_with(UNNAMED_TEMPLATE_PREFIX) || id.starts_with(VCC_TEMPLATE_PREFIX))
}

fn parse_format_version(json: &str) -> Option<(u32, u32)> {
    let (major, minor) = json.split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

#[allow(dead_code)]
pub fn serialize_alcom_template(template: AlcomTemplate) -> serde_json::Result<Vec<u8>> {
    let Some(base) = template.base.clone() else {
        return Err(serde_json::Error::custom(
            "Derived template serialization requires base",
        ));
    };

    // TODO: When we extended format, detect and update format_version
    let serialize = AlcomTemplateSerializeV1 {
        magic: MagicParser {
            ty: MAGIC.into(),
            format_version: "1.0".into(),
        },
        content: AlcomTemplateContentV1 {
            display_name: template.display_name,
            update_date: template.update_date,
            id: template.id.clone().map(TemplateId),
            base: TemplateId(base),
            unity_version: template.unity_version,
            vpm_dependencies: template.vpm_dependencies,
            unity_packages: template.unity_packages,
        },
    };
    serde_json::to_vec_pretty(&serialize)
}

pub fn serialize_alcom_project_archive_template(
    template: AlcomTemplate,
    archive_payload: &[u8],
) -> serde_json::Result<Vec<u8>> {
    let Some(archive) = template.archive else {
        return Err(serde_json::Error::custom(
            "Project archive template serialization requires archive metadata",
        ));
    };

    let serialize = AlcomTemplateSerializeV2 {
        magic: MagicParser {
            ty: MAGIC.into(),
            format_version: "2.0".into(),
        },
        content: AlcomTemplateContentV2 {
            display_name: template.display_name,
            update_date: template.update_date,
            id: template.id.clone().map(TemplateId),
            unity_version: template.unity_version,
            kind: AlcomTemplateKindContent::ProjectArchive { archive },
        },
    };
    let mut serialized = serde_json::to_vec_pretty(&serialize)?;
    serialized.push(0);
    serialized.extend_from_slice(archive_payload);
    Ok(serialized)
}

pub fn alcom_template_project_archive_payload(alcom_template: &[u8]) -> Option<&[u8]> {
    let json_end = alcom_template.iter().position(|&x| x == 0)?;
    let payload = &alcom_template[json_end + 1..];
    (!payload.is_empty()).then_some(payload)
}
