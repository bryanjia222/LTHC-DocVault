import typescriptEslint from "@typescript-eslint/eslint-plugin";
import typescriptParser from "@typescript-eslint/parser";
import vue from "eslint-plugin-vue";
import vueParser from "vue-eslint-parser";

export default [
  {
    ignores: ["dist/**", "node_modules/**", "src-tauri/target/**"],
  },
  ...vue.configs["flat/recommended"],
  {
    files: ["**/*.ts", "**/*.vue"],
    languageOptions: {
      parser: vueParser,
      parserOptions: {
        parser: typescriptParser,
        ecmaVersion: "latest",
        sourceType: "module",
      },
    },
    plugins: {
      "@typescript-eslint": typescriptEslint,
    },
    rules: {
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_" },
      ],
      // Formatting belongs to Prettier; ESLint owns code quality only. Every
      // vue rule below is purely stylistic and conflicts with Prettier output.
      "vue/array-bracket-spacing": "off",
      "vue/arrow-spacing": "off",
      "vue/attribute-hyphenation": "off",
      "vue/block-spacing": "off",
      "vue/block-tag-newline": "off",
      "vue/comma-dangle": "off",
      "vue/comma-spacing": "off",
      "vue/comma-style": "off",
      "vue/define-macros-order": "off",
      "vue/dot-location": "off",
      "vue/func-call-spacing": "off",
      "vue/html-closing-bracket-newline": "off",
      "vue/html-closing-bracket-spacing": "off",
      "vue/html-indent": "off",
      "vue/html-quotes": "off",
      "vue/html-self-closing": "off",
      "vue/key-spacing": "off",
      "vue/keyword-spacing": "off",
      "vue/max-attributes-per-line": "off",
      "vue/max-len": "off",
      "vue/multi-word-component-names": "off",
      "vue/multiline-html-element-content-newline": "off",
      "vue/mustache-interpolation-spacing": "off",
      "vue/no-multi-spaces": "off",
      "vue/no-spaces-around-equal-signs-in-attribute": "off",
      "vue/object-curly-newline": "off",
      "vue/object-curly-spacing": "off",
      "vue/object-property-newline": "off",
      "vue/operator-linebreak": "off",
      "vue/padding-line-between-blocks": "off",
      "vue/quote-props": "off",
      "vue/script-indent": "off",
      "vue/singleline-html-element-content-newline": "off",
      "vue/space-in-parens": "off",
      "vue/space-infix-ops": "off",
      "vue/space-unary-ops": "off",
      "vue/template-curly-spacing": "off",
    },
  },
];
