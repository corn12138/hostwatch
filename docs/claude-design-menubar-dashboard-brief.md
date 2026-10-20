# Hostwatch 极简菜单栏面板：设计任务

日期：2026-10-19。最新用户约束：**极简、简洁、高效、实用，让不懂电脑的人也能一眼看懂；这一轮直接设计完整产品所需的全部界面与交互。** 本文件取代此前偏多层、偏诊断工具的信息架构，也取代“只交付主面板”的范围限制；尚未实施产品代码或更改性能预算。

## 唯一核心目标

点击 macOS 菜单栏图标，几秒内知道：目前是否需要处理；如果需要，先做什么。

以 A 方案的清楚、克制与留白为唯一已选方向，直接深化完整设计，不再等待用户选方向。不把 A/B/C 拼成三个模式，不增加仪表盘、指标标签页或多级诊断导航。必要详情在同一面板内展开或切换，并提供明确返回；完整覆盖功能不等于增加首页复杂度。已有设计评审中的数据真实性问题仍需修正。

## 一个主面板

按阅读顺序只放：

1. **一句结论**：正常时“目前无需处理”；异常时直说“内存有些紧张”或“磁盘空间快满了”。缺数据时显示“暂时无法判断”，暂停时显示“监测已暂停”。不要宣称检查了所有电脑问题。
2. **三条简短状态**：内存、处理器、磁盘空间。用普通人能理解的状态词，必要数字作为辅助；正常不堆叠绿色勾选和重复解释。磁盘优先写“剩余 xx GB/GiB”，处理器可附轻量趋势。不要让用户先学会指标再判断状态。
3. **一条优先建议**：仅在需要时出现，说明最值得先做的一件事，并提供最多一个主要按钮。若有其他异常，可用“另有 N 项需要关注”渐进展开，不能隐藏严重问题。
4. **少量底部操作**：暂停/继续、更多。采样频率与退出等放入更多菜单；数据时间用简短文字显示。

主面板无需滚动即可完成基本判断。图表保留一至两个小趋势即可；只在帮助理解变化时出现，不增加大图、仪表盘、环形评分或动画装饰。数字与图表不能压过结论和行动。

## 用普通人的语言

| 避免作为首页文案 | 建议表达 |
| --- | --- |
| 内核内存压力 Warning | 内存有些紧张 |
| CPU saturation | 处理器持续繁忙 |
| swap / RSS / GiB/s | 首页不出现这些诊断术语；必要时在详情准确解释 |
| 证据—行动—复查三个大区块 | 一句原因、一条建议、一个主要按钮 |
| 无法预估可释放量等长说明 | 不做精确收益承诺即可，无需在首页反复声明 |
| 4 项检查均正常＋无告警＋无需处理 | 一句“目前无需处理” |

建议示例（仅为文案样例）：“内存有些紧张。可以先关闭暂时不用的应用。”需要展开时再给两三步清楚的操作说明。避免只把用户丢进陌生的系统工具；提供工具入口时说明进去后看哪里。用户手动处理后，系统继续观察，不能点击按钮就显示已恢复。

## 简单外观，准确判断

- 正常低调，异常突出；状态同时用文字和图形说明，不只靠颜色。
- 浅色清爽、文字清晰、留白适量；正文不靠很浅的灰色和很小的字号挤进面板。
- 原生 macOS 风格，无大面积装饰、持续动画或复杂导航。图表时间范围与缺口必须能理解。
- 不因内存占用高或已有交换文件就建议清理；不凭进程 RSS 大就认定它造成卡顿。
- 严重程度与数量必须准确：多项异常可以合成一条建议，但不能称其他相关指标都正常。
- 恢复条件读取现有策略，当前为连续 2 个正常样本，不改成设计稿中的 4 个。
- 交换读 6.2 + 写 18.4 MiB/s 超过当前严重阈值 16 MiB/s，样例等级须纠正。
- 样例中的时间、有效样本数、数据源数和容量单位须一致；所有演示数据明确标注。
- 复杂诊断信息仅在用户主动需要时渐进展开；第一版不要求新增方向性交换历史、压力历史带、5 分钟中位统计或 24 小时历史。

## 实现边界

复用现有 Rust 采集与告警核心、AppKit 菜单栏入口，评估轻量原生浮层。设计原型可以使用网页，但正式产品不因此引入 WebView、云端 AI、辅助进程或上传数据。

关闭面板停止不可见绘制，保持既定采样；打开面板时仍正常采样；显式暂停才停止采集。图表随样本更新，无持续动画。资源占用必须对最终版本重新测量，不沿用旧文字菜单的结果做保证。

默认采样 30 秒，历史上限 120 样本，实际可用时间范围由数据决定。保留 CLI 行为和已有告警规则。原 RSS 预算及 footprint 替代建议的决策状态不因设计改变。

## 本轮完整交付范围

用户已要求这一版直接完成所有必要页面与交互。不要停在首页、静态画板或再次等待选方向。沿用 A 的极简方向，覆盖下列全部界面；其中菜单、折叠内容、弹层无需硬拆成独立页面。

| 界面 / 表面 | 必须展示 | 完整交互 |
| --- | --- | --- |
| 菜单栏入口与主面板 | 一句结论、内存/处理器/磁盘三条状态、必要的小趋势、最优先建议、采样时间 | 图标打开/关闭、外部点击和 Escape 关闭、指标进入详情、建议进入处理方法、暂停/继续、更多 |
| 资源详情（共用模板，三种资源均需设计） | 内存：压力、用量及必要分项；处理器：用量趋势；磁盘：启动卷剩余空间与容量条；均有简短说明 | 从对应指标进入、查看趋势点/说明、返回主面板；样本不足和缺失有明确表现；磁盘不冒充目录扫描或 I/O 诊断 |
| 建议与处理方法 | 内存压力、交换活动、处理器繁忙、磁盘空间不足四类建议各有适用文案；一句原因、少量操作步骤、必要系统工具入口 | 查看对应建议、打开相关工具的结果反馈、处理后回来看观察状态、查看其他异常、返回；不自动清理或结束进程 |
| 按需进程查看 | 简单说明用途；可读取进程的 RSS 前五、名称、数据时间；说明占用较大不等于故障原因 | 用户主动刷新、加载、成功、空列表、读取失败/受限、再次刷新、返回；不要做进程管理控制台 |
| 设置与更多菜单 | 采样间隔、暂停/继续、简短指标说明/数据可用状态、退出；这些入口无需平铺在首页 | 打开/关闭菜单、修改间隔并反映当前选择、暂停和恢复状态同步、查看不可用原因、退出后的结束状态；未实现的通知/开机启动不提供可用开关 |
| 反馈与边界状态 | 工具无法打开、数据过期或失败、重试反馈、必要的轻提示 | 每个反馈提供可理解的下一步和返回/关闭路径；不新增无必要确认弹窗 |

资源详情和进程查看都属于主动展开的辅助功能，不占据首页主要空间。复用组件和布局，避免重复页面和深层路径。正常用户始终只需看主面板。

## 必须完整呈现的状态

- 主面板：首次加载、速率预热、正常、警告、严重、多项异常、部分指标不可用、整体采集失败、数据过期、暂停、恢复观察。
- 图表：有数据、历史不足、无历史、睡眠/失败缺口；按真实时间间隔显示，缺失不补零。
- 建议：无须处理、待处理、仍在观察、确认恢复；多个异常保留最高严重程度，合并建议不篡改异常项数。
- 进程查看：未刷新、刷新中、成功、无可读结果、失败/权限受限。
- 工具入口：准备打开、模拟成功反馈、无法打开；不能把网页原型的模拟当作真实 macOS 启动验证。
- 设置：间隔已选中、变更后生效、暂停时调整间隔、继续后采用新间隔。
- 外观与可访问性：全套必要界面的浅色/深色；键盘焦点、Escape、返回、文字可读性、状态不只靠颜色。收起主面板后采集仍运行，显式暂停才停止。

状态可以复用同一界面的组件变体和场景切换，不要求每个状态都新增一个导航页面。用于评审的演示场景控制放在原型外层，不混入正式产品界面。

## 可点击原型验收

本轮必须交付可点击原型，而不只是按钮外观：

1. 菜单栏打开 → 一眼读懂状态 → 关闭，再次打开。
2. 分别点内存/处理器/磁盘 → 对应详情 → 返回。
3. 异常 → 查看简短处理方法 → 系统工具入口的模拟反馈 → 返回 → 观察中/恢复；能切换至工具失败路径。
4. 多项异常 → 查看其余需要关注项 → 返回；不遗漏严重问题。
5. 按需进程查看 → 刷新 → 加载 → 结果；另覆盖空与失败路径。
6. 暂停 → 明确显示数据冻结和采样时间 → 调整间隔 → 继续 → 预热 → 新样本。
7. 设置/更多 → 修改间隔 → 所有相关位置一致；退出 → 展示原型中的“应用已退出”结束状态，而非真的关闭设计工具。
8. 数据不足/源失败/睡眠缺口 → 准确状态和简短解释 → 恢复后正确展示。

所有可见按钮、标签和可点击行必须有实际原型行为。确实不可执行的系统能力使用明确标注的模拟反馈，不能静默无响应，也不能假装连接真实机器。禁止为充数增加虚构功能。

## 交付物

- 唯一确定方向下的全套高保真必要界面、菜单与反馈状态；浅色/深色一致。
- 完整可点击原型，覆盖上述八条路径及异常分支。
- 一张简洁的界面与跳转清单；逐项标明入口、行为、反馈、返回或结束位置。
- 最少但够用的组件规范：尺寸、字体、色彩、间距、图表规则、焦点和键盘行为。
- 实现对照表：已有数据可直接支持、需新增 UI/状态暴露、仅为原型模拟。新指标不在本轮自行扩展。
- 自检记录：逐项写明哪些交互实际可点击、哪些系统动作仅模拟、哪些仍受工具限制。若当前工具无法完成某项，明确指出，不以静态图冒充交互完成。

始终用三个问题审查：不懂电脑的人能否几秒内看懂？能否知道下一步？删掉某个元素是否仍不影响判断？完整的是用户路径，简洁的是用户看到的界面。

## 可直接发给 Claude Design

```text
Proceed directly with the COMPLETE Hostwatch design and clickable prototype in
this round. The visual direction is already selected: simplify direction A.
Do not stop at the home screen, produce another three-option exploration, or
wait for another direction-selection approval.

The user's highest priority is minimalism, clarity, efficiency and practicality
for people who do not understand computers. Completeness means covering every
necessary interaction and state, not adding more navigation or dashboard widgets.

Create one compact native-macOS-style menu-bar product. The home popover contains
one plain-language conclusion, three concise resource rows (memory, processor,
disk space), at most one or two useful small trends, the highest-priority action
when needed, pause/resume and a More menu. Normal state is quiet. Use readable
Chinese text, restrained color and efficient whitespace. No large dashboards,
health scores, jargon-heavy home screen or nested diagnostic navigation.

Design ALL necessary surfaces:
1. Menu-bar icon and home popover, including open/close and dismissal behavior.
2. A shared resource-detail template, fully designed for memory, processor and
   disk, with simple explanations, appropriate data and a clear return path.
3. Recommendation/action details for memory pressure, swap activity, sustained
   processor load and low disk space. Give a simple reason, a few concrete manual
   steps and a relevant read-only system-tool entry point where useful.
4. On-demand process RSS top-five view, with refresh, sample age, loading, results,
   empty, failure and restricted-access states. Large RSS does not prove causation.
5. Settings and More menu: interval selection, pause/resume, concise metric/source
   explanations and quit. Do not add working switches for unimplemented features
   such as notifications or launch at login.
6. Feedback for tool-launch failure, unavailable data, retry and end states.
Reuse the same panel with disclosure or simple navigation; do not make each
state a new page. Ordinary users should only need the home popover.

Cover first loading, rate warmup, normal, warning, critical, multiple issues,
partial data, full collection failure, stale data, paused and recovering states.
Charts need normal data, insufficient history, empty history and sleep/failure
gaps. Provide coherent light/dark designs, keyboard focus, Escape, return paths,
readable secondary text and status indicators that do not rely only on color.
Keep prototype scenario controls outside the actual product interface.

Deliver real clickable prototype behavior for these journeys:
- Open menu-bar panel -> understand status -> close -> reopen.
- Each resource -> matching detail -> back.
- Alert -> simple handling instructions -> simulated system-tool outcome -> back
  -> observing/recovered; also show the tool-failure branch.
- Multiple alerts -> inspect additional issues -> back, preserving highest severity.
- Process view -> refresh -> loading -> results, including empty/failure branches.
- Pause -> frozen data/time -> change interval -> resume -> warmup -> fresh sample.
- Settings -> change interval -> consistent selected value everywhere; quit -> a
  prototype-only exited state, without closing the design application itself.
- Missing data or sleep gap -> accurate explanation -> correct recovery state.
Every visible interactive control must do something meaningful. System actions
that a web prototype cannot perform must show explicitly simulated feedback.
Do not leave inert buttons or pretend to read real machine data.

Preserve the existing product contract: recovery requires 2 normal samples, not
4; combined swap traffic of 24.6 MiB/s is critical at the current 16 MiB/s threshold;
a single merged recommendation can cover multiple abnormal resource checks.
Missing data is unknown, never healthy or zero. Do not suggest cleaning based on
high RAM usage or historical swap alone, automatically clean or terminate anything,
or promise speedups. Clearly label illustrative data. Use consistent units and
sample times. Do not invent unavailable metrics or 24-hour history.
Closing the panel stops invisible rendering, not collection; explicit pause stops
collection. The native implementation will reuse the Rust collector and AppKit
shell; the web prototype is a design reference, not a production stack decision
or proof of native integration/performance.

Deliver all necessary high-fidelity screens and states, a complete clickable
prototype, a concise navigation/interaction map, reusable component specs and a
capability-gap list. Check every journey yourself and report actual clickable
behavior, simulated system actions and any tool limitations separately.
If you have file access, read docs/claude-design-menubar-dashboard-brief.md fully.
The brief is complete: continue through all scoped deliverables in this round.
```
