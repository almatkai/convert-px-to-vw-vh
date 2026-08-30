/// <reference types="@raycast/api">

/* 🚧 🚧 🚧
 * This file is auto-generated from the extension's manifest.
 * Do not modify manually. Instead, update the `package.json` file.
 * 🚧 🚧 🚧 */

/* eslint-disable @typescript-eslint/ban-types */

type ExtensionPreferences = {}

/** Preferences accessible in all the extension's commands */
declare type Preferences = ExtensionPreferences

declare namespace Preferences {
  /** Preferences accessible in the `index` command */
  export type Index = ExtensionPreferences & {}
  /** Preferences accessible in the `height` command */
  export type Height = ExtensionPreferences & {}
  /** Preferences accessible in the `width` command */
  export type Width = ExtensionPreferences & {}
  /** Preferences accessible in the `menu-bar` command */
  export type MenuBar = ExtensionPreferences & {}
}

declare namespace Arguments {
  /** Arguments passed to the `index` command */
  export type Index = {}
  /** Arguments passed to the `height` command */
  export type Height = {
  /** height */
  "height": string,
  /** pixels */
  "pixels": string
}
  /** Arguments passed to the `width` command */
  export type Width = {
  /** width */
  "width": string,
  /** pixels */
  "pixels": string
}
  /** Arguments passed to the `menu-bar` command */
  export type MenuBar = {}
}

