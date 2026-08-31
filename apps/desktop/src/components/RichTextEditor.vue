<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import tinymce from "tinymce/tinymce";
import type { Editor as TinyMceEditor } from "tinymce/tinymce";
import "tinymce/themes/silver";
import "tinymce/icons/default";
import "tinymce/models/dom";
import "tinymce/plugins/advlist";
import "tinymce/plugins/charmap";
import "tinymce/plugins/code";
import "tinymce/plugins/emoticons";
import "tinymce/plugins/emoticons/js/emojis";
import "tinymce/plugins/fullscreen";
import "tinymce/plugins/insertdatetime";
import "tinymce/plugins/lists";
import "tinymce/plugins/nonbreaking";
import "tinymce/plugins/pagebreak";
import "tinymce/plugins/preview";
import "tinymce/plugins/table";
import "tinymce/plugins/wordcount";
import "tinymce/skins/ui/oxide/skin.min.css";
import contentStyles from "tinymce/skins/content/default/content.min.css?raw";
import wangZhiGangFont from "../assets/fonts/WangZhiGangTi.ttf";
import "tinymce-i18n/langs6/zh-Hans.js";

const props = defineProps<{
  modelValue: string;
  placeholder?: string;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string];
}>();

const { locale, t } = useI18n();
const editorHost = ref<HTMLTextAreaElement | null>(null);
let editor: TinyMceEditor | null = null;
let disposed = false;

// The editor renders in an iframe, so the bundled handwriting font has to be
// registered inside that document in addition to the app-wide @font-face.
const editorContentStyles = [
  contentStyles,
  `@font-face { font-family: "汪志刚体"; src: url("${wangZhiGangFont}") format("truetype"); }`,
].join("\n");

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
    language: locale.value === "zh-CN" ? "zh-Hans" : undefined,
    branding: false,
    promotion: false,
    placeholder: props.placeholder,
    min_height: 260,
    resize: false,
    statusbar: true,
    convert_urls: false,
    relative_urls: false,
    remove_script_host: false,
    content_style: editorContentStyles,
    contextmenu: "copy paste | inserttable | cell row column deletetable",
    paste_convert_word_fake_lists: false,
    paste_webkit_styles: "all",
    paste_merge_formats: true,
    paste_auto_cleanup_on_paste: false,
    paste_data_images: false,
    automatic_uploads: false,
    toolbar_mode: "floating",
    plugins:
      "advlist charmap code emoticons fullscreen " +
      "insertdatetime lists nonbreaking pagebreak " +
      "preview table wordcount",
    toolbar:
      "bold italic underline strikethrough | forecolor | fontfamily fontsize | more",
    font_size_formats: "12px 14px 16px 18px 24px 36px 48px 56px 72px",
    font_family_formats:
      "汪志刚体=汪志刚体;" +
      "思源黑体=Source Han Sans;微软雅黑=Microsoft YaHei,Helvetica Neue,PingFang SC,sans-serif;" +
      "苹果苹方=PingFang SC,Microsoft YaHei,sans-serif;宋体=simsun,serif;" +
      "仿宋体=FangSong,serif;黑体=SimHei,sans-serif;楷体=楷体_GB2312,SimKai;" +
      "Arial=arial,helvetica,sans-serif;Arial Black=arial black,avant garde;" +
      "Book Antiqua=book antiqua,palatino;",
    table_use_colgroups: true,
    setup: (instance) => {
      editor = instance;
      instance.ui.registry.addGroupToolbarButton("more", {
        icon: "more-drawer",
        tooltip: t("qinbixin.editorMoreTools"),
        items:
          "undo redo cut copy paste pastetext | " +
          "subscript superscript removeformat | backcolor blocks lineheight | " +
          "alignleft aligncenter alignright alignjustify outdent indent | " +
          "bullist numlist blockquote | " +
          "table charmap emoticons hr pagebreak insertdatetime | " +
          "code preview fullscreen",
      });
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

<style>
/*
 * Group-toolbar popups render in TinyMCE's aux sink, outside this component's
 * scoped DOM. Stack the popup vertically so each `|` group in the "more"
 * button's items becomes its own row instead of one very wide toolbar.
 */
.tox.tox-tinymce-aux .tox-toolbar__overflow {
  flex-direction: column;
}
</style>
