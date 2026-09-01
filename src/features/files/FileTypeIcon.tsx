import archive from "../../assets/file-type-icons/zip.svg?url";
import audio from "../../assets/file-type-icons/audio.svg?url";
import certificate from "../../assets/file-type-icons/certificate.svg?url";
import database from "../../assets/file-type-icons/database.svg?url";
import document from "../../assets/file-type-icons/document.svg?url";
import docker from "../../assets/file-type-icons/docker.svg?url";
import email from "../../assets/file-type-icons/email.svg?url";
import executable from "../../assets/file-type-icons/exe.svg?url";
import font from "../../assets/file-type-icons/font.svg?url";
import git from "../../assets/file-type-icons/git.svg?url";
import go from "../../assets/file-type-icons/go.svg?url";
import html from "../../assets/file-type-icons/html.svg?url";
import image from "../../assets/file-type-icons/image.svg?url";
import java from "../../assets/file-type-icons/java.svg?url";
import javascript from "../../assets/file-type-icons/javascript.svg?url";
import json from "../../assets/file-type-icons/json.svg?url";
import key from "../../assets/file-type-icons/key.svg?url";
import library from "../../assets/file-type-icons/dll.svg?url";
import license from "../../assets/file-type-icons/license.svg?url";
import lock from "../../assets/file-type-icons/lock.svg?url";
import log from "../../assets/file-type-icons/log.svg?url";
import markdown from "../../assets/file-type-icons/markdown.svg?url";
import pdf from "../../assets/file-type-icons/pdf.svg?url";
import php from "../../assets/file-type-icons/php.svg?url";
import powerpoint from "../../assets/file-type-icons/powerpoint.svg?url";
import powershell from "../../assets/file-type-icons/powershell.svg?url";
import python from "../../assets/file-type-icons/python.svg?url";
import react from "../../assets/file-type-icons/react.svg?url";
import ruby from "../../assets/file-type-icons/ruby.svg?url";
import rust from "../../assets/file-type-icons/rust.svg?url";
import settings from "../../assets/file-type-icons/settings.svg?url";
import shell from "../../assets/file-type-icons/console.svg?url";
import spreadsheet from "../../assets/file-type-icons/table.svg?url";
import stylesheet from "../../assets/file-type-icons/css.svg?url";
import toml from "../../assets/file-type-icons/toml.svg?url";
import typescript from "../../assets/file-type-icons/typescript.svg?url";
import video from "../../assets/file-type-icons/video.svg?url";
import word from "../../assets/file-type-icons/word.svg?url";
import xml from "../../assets/file-type-icons/xml.svg?url";
import yaml from "../../assets/file-type-icons/yaml.svg?url";
import { resolveFileTypeIcon, type FileTypeIconName } from "./file-type-icon";

const iconAssets: Readonly<Record<FileTypeIconName, string>> = {
  archive,
  audio,
  certificate,
  database,
  document,
  docker,
  email,
  executable,
  font,
  git,
  go,
  html,
  image,
  java,
  javascript,
  json,
  key,
  library,
  license,
  lock,
  log,
  markdown,
  pdf,
  php,
  powerpoint,
  powershell,
  python,
  react,
  ruby,
  rust,
  settings,
  shell,
  spreadsheet,
  stylesheet,
  toml,
  typescript,
  video,
  word,
  xml,
  yaml,
};

export function FileTypeIcon({ fileName }: { fileName: string }) {
  return <img
    alt=""
    aria-hidden="true"
    className="h-[17px] w-[17px] shrink-0 object-contain"
    draggable={false}
    src={iconAssets[resolveFileTypeIcon(fileName)]}
  />;
}
