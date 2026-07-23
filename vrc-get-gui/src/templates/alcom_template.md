# ALCOMD3 Template

This module handles the new `.alcomtemplate` file format.

## Goals

The goal of the .alcomtemplate file at this moment are:

- Easy way to create custom templates based on VRChat Worlds / Avatars template  
  Customization generally consists of:
    - Adding new package to the template
    - importing unitypackage
- Easy way to distribute / share your own custom template
- Preserve complete Unity project directory templates imported from legacy VCC
  data without keeping a separate VCC-style template directory as ALCOMD3's
  primary storage format.

## Structure

The following json is jsonc but in real file comments are not allowed.

```json5
{
  // magic to not recognize other file as project template
  "$type": "com.anatawa12.vrc-get.custom-template",
  // The format version. major part will be incremented when the format is changed incompatible way.
  // The minor part will be incremented when the format is updated compatible way, in other words,
  // older implementation can create project with new format with some loss on project
  // E.g. If new template format allows no-base template, it would be v2. 
  "formatVersion": "1.0",
  // The display name of template
  "displayName": "NDMF Tools With Anon",
  // The date when the template was updated
  // This is an optional field
  "updateDate": "2025-04-17T00:00:00",
  // Optional id of the package.
  // If null the package won't be a base package of other package.
  // Internally some id will be assgined in form of `com.anatawa12.vrc-get.user.<uuid>` like 
  // `com.anatawa12.vrc-get.user.023274af4b31477d9ad6c69b5123adc6` but it won't be used as base id.
  // This field can only use Portable Filename Character Set in POSIX, i.e. /[a-zA-Z0-9._-]+/
  "id": null,
  // The base template. Currently you must have some base template.
  // Currently built-in base templates are:
  // - com.anatawa12.vrc-get.vrchat.avatars   - VRChat Avatars SDK3
  // - com.anatawa12.vrc-get.vrchat.worlds    - VRChat Worlds SDK3
  // - com.anatawa12.vrc-get.blank            - Completely blank project
  "base": "com.anatawa12.vrc-get.vrchat.worlds",
  // The supported unity version for the project.
  // It would be in form of semver range format.
  // The unity version channel part and increment part will be ignored.
  // This cannot override base templates versions, so even if you specified `2022.x.x`, 
  // since com.anatawa12.vrc-get.vrchat.worlds only supports '2022.3.22' and '2022.3.6',
  // this templete can only be used with '2022.3.22' and '2022.3.6'.
  "unityVersion": "2022.x.x",
  // The packages to be installed
  // If the same package is speciifed in base and current, versions matches both range will be used.
  // This is optional field; if omitted no packages are imported (addition to base)
  "vpmDependencies": {
    "com.anatawa12.avatar-optimizer": "1.x",
    "nadena.dev.modular-avatar": "1.x",
    "net.rs64.tex-trans-tool": "0.9.x"
  },
  // The unitypackages to be installed.
  // This is an optional field; if omitted no packages are imported (addition to base)
  "unityPackages": [
    "/Users/anatawa12/UnityPackages/Anon.unitypackage"
  ]
}
```

## Project Archive Template

Format version `2.0` adds `templateKind`. The `projectArchive` kind stores a
complete Unity project directory as a `tar.gz` payload appended after the JSON
header and a null byte (`\0`). It is used when importing legacy VCC directory
templates into ALCOMD3.

```json5
{
  "$type": "com.anatawa12.vrc-get.custom-template",
  "formatVersion": "2.0",
  "displayName": "Imported VCC Template",
  "updateDate": "2026-06-23T00:00:00Z",
  "id": "com.alcomd3.imported.vcc.imported-vcc-template.0123456789ab",
  "templateKind": "projectArchive",
  "archive": {
    "format": "tar.gz",
    "unityVersion": "2022.3.22f1"
  }
}
```

Archive templates are ALCOMD3 templates. They are stored under
`templates/` with the `.alcomtemplate` extension, can be exported and
removed like other ALCOMD3 templates, and can be used as a base for derived
templates. They are not written back to VCC's `Templates` directory.
