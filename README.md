# PPTUI for Zed

<img src="logo.svg" alt="PPTUI logo" width="112" height="112" />

给 `.pptui` 幻灯片格式用的 Zed 扩展：语法高亮、缩进、大纲，加上**内置 XSD 的默认 schema 校验与补全**（LemMinX）。

## 功能

- 语言 `PPTUI`，匹配 `*.pptui` 与 `*.pptui.xml`
- 基于 [tree-sitter-xml](https://github.com/tree-sitter-grammars/tree-sitter-xml) 的高亮 / 缩进 / 大纲
- 内置 `schemas/pptui.xsd`（编译进扩展，启动时释放到工作目录），通过 `xml.fileAssociations` 默认关联，用户零配置
- LemMinX 二进制：优先 `lsp.pptui-lemminx.binary.path` → `PATH` 里的 `lemminx` → 自动从 [vscode-xml releases](https://github.com/redhat-developer/vscode-xml/releases) 下载

## 本地安装（开发）

1. 命令面板执行 `zed: extensions`
2. 右上角 **Install Dev Extension**，选择本目录
3. 打开任意 `.pptui` 文件；首次使用会下载 LemMinX，状态在语言服务器菜单里可见

需要 Rust 的 `wasm32-wasip2` target：

```bash
rustup target add wasm32-wasip2
```

## 覆盖默认配置

默认值只是兜底，用户配置按键合并、数组追加：

```json
{
  "lsp": {
    "pptui-lemminx": {
      "settings": {
        "xml": {
          "validation": { "enabled": true },
          "fileAssociations": [
            { "pattern": "**/*.pptui", "systemId": "/absolute/path/to/custom.xsd" }
          ]
        }
      }
    }
  }
}
```

想用 XML catalog 管理多套 schema 也可以，`xml.catalogs` 与 `xml.fileAssociations` 都原样透传给 LemMinX。


## 排错

- `zed: open log` 看扩展与 LSP 启动日志；`zed --foreground` 看更详细的 INFO 日志
- 校验不生效：确认文件被识别为 `PPTUI`（状态栏语言选择器），且 `lsp.pptui-lemminx.settings.xml.validation.enabled` 没有被关掉
- 改了 `lsp.*` 设置后需要重启语言服务器（`editor: restart language server`），LemMinX 只在启动时读设置
- 报 `schema_reference.4 ... could not find the document`：schema 没释放到工作目录。工作目录是 `…/Zed/extensions/work/pptui`，**不包含**扩展源码，所以 XSD 必须由扩展自己写出；重新 Install Dev Extension 后重启 LSP 即可

## 示例

`examples/hello.pptui` 是一个通过 XSD 校验的最小 deck，可直接用 Zed 打开验证高亮与补全：

```xml
<Deck Width="12192000" Height="6858000">
  <Meta Title="PPTUI Hello" Author="long" />
  <Slide Name="Intro">
    <StackPanel Margin="40">
      <TextBlock FontSize="48" FontWeight="Bold">Hello, PPTUI</TextBlock>
    </StackPanel>
  </Slide>
</Deck>
```
