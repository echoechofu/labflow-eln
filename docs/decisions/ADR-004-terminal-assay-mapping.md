# ADR-004: 终末检测 Mapping 与 Sample lineage 分离

## Status

Accepted

## Context

qPCR、ELISA、CCK-8 都需要表达选用的 Sample、检测项目、孔板分配和仪器原始读数。`Sample × Target` 是一次检测中的孔位含义，不是新的生物材料；technical replicate 又可以由同一组合占用的孔数直接推导。旧的 qPCR 专属孔位表以 Experiment + position 唯一，并固化重复序号，无法安全表达同一 Experiment 的多块板或复用到其他检测。

## Decision

建立独立的通用终末检测层：Record Setup 保存 Assay Items；Assay Plate 保存板边界；Well Mapping 保存 `Well → Sample × AssayItem`；Raw Import 保存原始 Attachment 与 `Well → Measurement`。Mapping 与 Raw 没有先后依赖，最终只通过同一 plate/well 形成 join dataset。

这些实体不创建 Sample、不写 Sample relation，也不作为 ProcessEvent output。terminal assay 的 ProcessEvent 仅记录输入 Sample 与 usage。technical replicate 不持久化，按映射孔数派生。

通用层仍只负责 Setup、Mapping、Raw 与 join，不内置具体检测算法。在其后，qPCR 已增加专属两层 Analysis：ΔCt 保存基因角色、Well inclusion/QC，并对每个 Target × 内参独立计算；可选 ΔΔCt 从其中选择单一内参，再保存 Sample 分组、共同/对应对照关系和 relative expression。分析不创建 Sample，也不写通用 `results` 表。

原始文件通用格式为 UTF-8 CSV/TSV/TXT；qPCR 另外支持从 XLSX 中查找 `Well` 与 Cq 列。旧 `qpcr_plate_wells` 保留兼容，不自动猜测迁移到新模型。

## Consequences

- qPCR、ELISA、CCK-8 可以复用同一 Mapping/Raw 数据层。
- 多块板可安全重复使用 A01 等孔位。
- 未映射的 raw well 不进入 join dataset，但原始文件和解析行仍保留。
- qPCR Analysis 读取稳定 join dataset，不修改 Record Setup、Mapping、Raw 或 Sample lineage；ELISA/CCK-8 仍可在相同边界后增加各自算法。
- qPCR XLSX 支持以列名识别为边界，不等同于已经实现所有厂商专属格式；blank/standard 的完整 UI、统计和绘图仍未实现。
- 错建的空白 Plate 可以删除，但 command 层必须确认它既无 Mapping、也无 Raw Import；已有检测数据不做级联删除。

## Alternatives considered

- 为每种 Protocol 开发专属孔板表和页面：拒绝，会重复实现相同的板/孔基础能力。
- 将 Sample × Target 建成派生 Sample：拒绝，它是测量映射，不是新的材料身份。
- 在 Setup 中保存 technical replicate 数：拒绝，实际映射孔数才是事实来源。
- 直接扩展旧 `qpcr_plate_wells`：拒绝，其 Record/Plate 边界和唯一键不足以承载通用多板模型。
