/// <reference types="@raycast/api">

/* 🚧 🚧 🚧
 * This file is auto-generated from the extension's manifest.
 * Do not modify manually. Instead, update the `package.json` file.
 * 🚧 🚧 🚧 */

/* eslint-disable @typescript-eslint/ban-types */

type ExtensionPreferences = {
  /** Actions Before Pasting - Trim spaces in front of plain text */
  "trimStart": boolean,
  /** undefined - Trim spaces at the end of plain text */
  "trimEnd": boolean,
  /** undefined - Replace all line breaks with spaces */
  "cleanLineBreaks": boolean,
  /** Actions When Pasting - Auto fetch title from URL when pasting as Markdown Link and Markdown Image */
  "autoFetchTitle": boolean,
  /** Actions After Pasting - Replace clipboard with pasted text after pasting */
  "replaceClipboard": boolean,
  /** undefined - Show tips after pasting */
  "showTips": boolean
}

/** Preferences accessible in all the extension's commands */
declare type Preferences = ExtensionPreferences

declare namespace Preferences {
  /** Preferences accessible in the `paste-as-plain-text` command */
  export type PasteAsPlainText = ExtensionPreferences & {}
}

declare namespace Arguments {
  /** Arguments passed to the `paste-as-plain-text` command */
  export type PasteAsPlainText = {
  /** Plain Text */
  "advancedPasteFormat": "URL" | "File" | "JSON" | "Number" | "Markdown Link" | "Markdown Image"
}
}

