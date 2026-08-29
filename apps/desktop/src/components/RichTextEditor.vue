<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";

import tinymce from "tinymce/tinymce";
import type { Editor as TinyMceEditor } from "tinymce/tinymce";
import "tinymce/themes/silver";
import "tinymce/icons/default";
import "tinymce/models/dom";
import "tinymce/plugins/advlist";
import "tinymce/plugins/anchor";
import "tinymce/plugins/autolink";
import "tinymce/plugins/charmap";
import "tinymce/plugins/code";
import "tinymce/plugins/emoticons";
import "tinymce/plugins/emoticons/js/emojis";
import "tinymce/plugins/fullscreen";
import "tinymce/plugins/image";
import "tinymce/plugins/insertdatetime";
import "tinymce/plugins/link";
import "tinymce/plugins/lists";
import "tinymce/plugins/media";
import "tinymce/plugins/nonbreaking";
import "tinymce/plugins/pagebreak";
import "tinymce/plugins/preview";
import "tinymce/plugins/table";
import "tinymce/plugins/wordcount";
import "tinymce/skins/ui/oxide/skin.min.css";
import contentStyles from "tinymce/skins/content/default/content.min.css?raw";

import { isTauri } from "../composables/useVault";

interface UploadedMedia {
  url: string;
  title: string;
}

const props = defineProps<{
  modelValue: string;
  placeholder?: string;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string];
}>();

const editorHost = ref<HTMLTextAreaElement | null>(null);
let editor: TinyMceEditor | null = null;
let disposed = false;

const UPLOAD_ACCEPT: Record<string, string> = {
  image: "image/*",
  media: "video/*,audio/*",
  file: [
    ".pdf",
    ".txt",
    ".zip",
    ".rar",
    ".7z",
    ".doc",
    ".docx",
    ".xls",
    ".xlsx",
    ".ppt",
    ".pptx",
    ".mp3",
    ".mp4",
  ].join(","),
};

function uploadTypeForPicker(fileType: string): number {
  if (fileType === "image") return 0;
  if (fileType === "media") return 2;
  return 1;
}

async function uploadFile(
  file: File | Blob,
  uploadType: number,
  fileName?: string,
): Promise<string> {
  if (!isTauri()) return URL.createObjectURL(file);
  const resolvedName = fileName || (file as File).name || "attachment";
  const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
  const result = await invoke<UploadedMedia>("qinbixin_upload_bytes", {
    fileName: resolvedName,
    bytes,
    uploadType,
  });
  return result.url;
}

async function pickAndUpload(
  callback: (url: string, meta?: { text?: string; title?: string }) => void,
  fileType: string,
): Promise<void> {
  const input = document.createElement("input");
  input.type = "file";
  input.accept = UPLOAD_ACCEPT[fileType] || "*/*";
  input.onchange = () => {
    const file = input.files?.[0];
    if (!file) return;
    void uploadFile(file, uploadTypeForPicker(fileType), file.name)
      .then((url) =>
        callback(url, {
          title: file.name,
          text: fileType === "file" ? file.name : undefined,
        }),
      )
      .catch((error) => {
        console.error("Qinbixin rich media upload failed", error);
        window.alert(String(error));
      });
  };
  input.click();
}

function emitUpdate(): void {
  if (!editor) return;
  const value = editor.getContent();
  if (value !== props.modelValue) {
    emit("update:modelValue", value);
  }
}

onMounted(async () => {
  await nextTick();
  if (!editorHost.value || disposed) return;
  await tinymce.init({
    target: editorHost.value,
    menubar: false,
    skin: false,
    content_css: false,
    branding: false,
    promotion: false,
    placeholder: props.placeholder,
    min_height: 260,
    resize: false,
    statusbar: true,
    convert_urls: false,
    relative_urls: false,
    remove_script_host: false,
    content_style: contentStyles,
    contextmenu:
      "copy paste | link image inserttable | cell row column deletetable",
    image_caption: true,
    image_dimensions: false,
    paste_convert_word_fake_lists: false,
    paste_webkit_styles: "all",
    paste_merge_formats: true,
    paste_auto_cleanup_on_paste: false,
    paste_data_images: true,
    automatic_uploads: true,
    file_picker_types: "image media file",
    plugins:
      "advlist anchor autolink charmap code emoticons fullscreen " +
      "image insertdatetime link lists media nonbreaking pagebreak " +
      "preview table wordcount",
    toolbar:
      "undo redo | cut copy paste pastetext | " +
      "bold italic underline strikethrough subscript superscript removeformat | " +
      "forecolor backcolor | blocks fontfamily fontsize lineheight | " +
      "alignleft aligncenter alignright alignjustify outdent indent | " +
      "bullist numlist blockquote | link anchor media image | " +
      "table charmap emoticons | hr pagebreak insertdatetime | " +
      "code preview fullscreen",
    font_size_formats: "12px 14px 16px 18px 24px 36px 48px 56px 72px",
    font_family_formats:
      "思源黑体=Source Han Sans;微软雅黑=Microsoft YaHei,Helvetica Neue,PingFang SC,sans-serif;" +
      "苹果苹方=PingFang SC,Microsoft YaHei,sans-serif;宋体=simsun,serif;" +
      "仿宋体=FangSong,serif;黑体=SimHei,sans-serif;楷体=楷体_GB2312,SimKai;" +
      "Arial=arial,helvetica,sans-serif;Arial Black=arial black,avant garde;" +
      "Book Antiqua=book antiqua,palatino;",
    table_use_colgroups: true,
    images_upload_handler: async (blobInfo, progress) => {
      try {
        const blob = blobInfo.blob();
        const file =
          blob instanceof File
            ? blob
            : new File([blob], blobInfo.filename(), { type: blob.type });
        return await uploadFile(file, 0, blobInfo.filename());
      } finally {
        progress(100);
      }
    },
    file_picker_callback: (callback, _value, meta) => {
      void pickAndUpload(callback, meta.filetype);
    },
    setup: (instance) => {
      editor = instance;
      instance.on("init input change undo redo SetContent", emitUpdate);
    },
    init_instance_callback: (instance) => {
      instance.setContent(props.modelValue || "");
      if (props.disabled) instance.mode.set("readonly");
    },
  });
});

watch(
  () => props.modelValue,
  (value) => {
    if (!editor || editor.destroyed || editor.getContent() === value) return;
    editor.setContent(value || "");
  },
);

watch(
  () => props.disabled,
  (disabled) => {
    editor?.mode.set(disabled ? "readonly" : "design");
  },
);

onBeforeUnmount(() => {
  disposed = true;
  editor?.destroy();
  editor = null;
});
</script>

<template>
  <textarea
    ref="editorHost"
    class="rich-editor-host"
    :value="props.modelValue"
  />
</template>

<style scoped>
.rich-editor-host {
  display: none;
}

.rich-editor-host + .tox-tinymce {
  border-color: var(--border-strong);
  border-radius: var(--radius-sm);
  overflow: hidden;
}
</style>
