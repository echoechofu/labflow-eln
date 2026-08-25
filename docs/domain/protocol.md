# Protocol

## 当前内置模型

Protocol 由 `protocols` 与 `protocol_versions` 表表达。每个内置 Protocol 的活跃版本包含 JSON schema，当前使用的字段包括：

- `blocks`：UI 的步骤概览；
- `fields`：文本、数字、选择、Sample 选择和孔板布局字段，以及必填/可见性信息；
- `template` 或 `templateSelector` + `templateVariants`：Record 正文模板；
- `execution`：事件类型、允许的输入类型/来源/基数、输出类型与模式、Sample usage policy、Result 类型。

内置 catalog 当前有 11 个 Protocol：细胞复苏、细胞传代、细胞铺板、细胞加刺激、RNA Extraction — Trizol、Reverse Transcription — PrimeScript、SYBR Green qPCR、Western Blot、培养上清收集、ELISA — 细胞因子、CCK-8 细胞增殖/毒性实验。ELISA 与 CCK-8 的实验正文来自仓库根目录的 `组内protocol整理_Ver1.0.doc`。

创建 Record 时，执行器校验必填字段，按任务日期渲染模板，将 schema snapshot 和渲染后的正文写入 Record；随后按 execution rule 创建 ProcessEvent、Sample usage、输出 Sample 或 Result。

qPCR、ELISA、CCK-8 的 schema 还包含当前已实现的 `terminalAssay` 描述：检测项目 UI 名称、raw metric 和允许板型。Record 创建时保存 Assay Items；随后共用独立 Plate Mapping/Raw Data 功能层。这三类 Protocol 不在 Setup 时创建 pending Result。当前只有 qPCR 在共用 join dataset 之后实现了专属 ΔCt/ΔΔCt Analysis；ELISA、CCK-8 尚无计算层。

## 当前专属逻辑与通用边界

execution JSON 已承载一部分通用规则（`inputTypes`、`inputSource`、`inputCardinality`、`outputType`、`outputMode`、`resultTypes`、`consumptionPolicy`），但当前不是完整的通用 DSL。Rust 执行器仍含有事件类型分支，例如：

- `thaw`、`passage`、`plating`、`treatment` 的输入/新建对象校验；
- 孔板刺激分组、支持的孔板规格、容量及孔位分配；
- `one`、`count`、`per_input`、`plate_or_dish`、`plate_wells`、`none` 输出模式；
- `cDNA` 的显示后缀与 Sample 类型大写规范化。

因此，不应把当前 Protocol schema 描述为“任意用户上传即可安全执行”的声明式系统。内置 Protocol 的 ID、event type 和部分字段名仍与 Rust 逻辑协作。

## Protocol 与历史 Record

Protocol 的活跃版本可在启动时随内置 catalog 升级；新的 schema version 会插入 `protocol_versions` 并更新 `active_version`。创建的 Record 则保存当时 schema 的副本和渲染后正文。历史 Record 的展示和 export 读取自身保存的数据，不重新读取 active Protocol 模板。

## Future design constraints（尚未实现）

- 用户自定义 Protocol 必须同时表达记录模板、字段和 Sample 输入/输出语义，不能只上传 Word/PDF。
- 将用户定义内容解释为执行逻辑时需要验证与安全边界；当前没有允许用户执行任意 JavaScript、Rust 或 SQL 的机制。
- 若引入通用 schema/导入器，应先迁移内置 Protocol，保持既有 Record snapshot 和 lineage 数据可读。
