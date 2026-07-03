# Fitsh 智能合约语言手册

面向有智能合约开发经验（如 Solidity）的开发者，帮助快速上手并开发 Fitsh 合约的实用指南。

---

## 1. 设计目标、特点及与 Solidity 的对比

### 1.1 设计目标

- **栈式虚拟机**：Fitsh 编译为在 Hacash VM 上执行的栈式字节码
- **确定性执行**：相同输入产生相同的 gas 和正确性输出
- **Gas 计费**：每条操作消耗 gas，用于链上资源核算
- **IR 优先编译**：源码 → IR（中间表示）→ 字节码；IR 是主要编译目标

### 1.2 核心特点

| 特性 | 说明 |
|------|------|
| **IR 反编译** | IR 字节码可反编译回可读的 Fitsh 源码（可往返稳定） |
| **函数选择** | 4 字节名称哈希；无重载；同名即同选择器 |
| **库绑定** | `lib Name = idx [: address]`；按索引，可选部署地址 |
| **继承** | 通过 `inherit` 组合式继承；无 Solidity 风格类继承 |
| **抽象钩子** | 通过 `abstract` 定义系统支付钩子（如 PayableHACD、PayableAsset） |

### 1.3 与 Solidity 对比

| 方面 | Fitsh | Solidity |
|------|-------|----------|
| 继承 | `inherit`（组合） | 使用 `is` 的类继承 |
| 修饰符 | 无；使用 `assert`/`if` | `modifier` 关键字 |
| 整数类型 | `u8`、`u16`、`u32`、`u64`、`u128` | `uint8`..`uint256`、`int` |
| 字符串/字节 | `bytes`（引号字符串和十六进制） | `string`、`bytes` |
| 参数 | `param { a b c }` 解包到槽位 | 在函数签名中声明 |
| 状态可变性 | `callextview`/`callusepure` 只读 | `view`/`pure` 修饰符 |
| 底层调用 | `codecall`（源码层 `end` 可省略；原地执行） | `delegatecall` |
| 支付钩子 | `abstract PayableHACD` 等 | `receive()`、`fallback()` |

---

## 2. IR 反编译输出（核心特性）

### 2.1 什么是 IR 反编译

Fitsh 编译为 IR 字节码。该 IR 可通过 `format_ircode_to_lang` 或 `ircode_to_lang` **反编译回 Fitsh 源码**。反编译输出可读，在合适选项下可重新编译为字节一致的字节码。

### 2.2 为何重要

- **审计**：在无原始源码时检查已编译合约
- **调试**：理解 VM 实际执行内容
- **链上验证**：验证已部署字节码与源码一致

### 2.3 反编译选项

| 选项 | 效果 |
|------|------|
| `trim_param_unpack` | 推断参数名时输出 `param { $0 $1 ... }` |
| `hide_default_call_argv` | 无参数时省略 `nil` 或 `""` 占位符 |
| `call_short_syntax` | 影响 `call ... ext(i).sig(...)`/`codecall` 中库索引是否用库名显示 |
| `flatten_array_list` | 输出 `[a, b, c]` 而非 `list { a b c }` |
| `flatten_syscall_cat` | 展开系统调用参数中的嵌套 `++` |
| `recover_literals` | 恢复并输出数字/字节字面量 |

### 2.4 输出形式

- **参数**：`param { owner amount fee }` 或名称不可用时 `param { $0 $1 $2 }`
- **调用**：统一输出 `call <effect> <target>.<sig>(...)`，例如 `call view ext(1).0xabcdef01(addr)`
- **代码拼接**：统一输出 `codecall <libidx>.<sig>`，例如 `codecall 1.0xabcdef01`

---

## 3. 合约结构（`contract` 关键字）

### 3.1 顶层语法

```fitsh
pragma fitsh 1.0.0

contract ContractName {

    deploy {
        protocol_cost: amount("1:248"),
        nonce: 1u32,
        construct_argv: 0xaabb2244
    }

    library [
        Lib1: emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS,
        Lib2: bJKaNA2dLGxJEwp3xSok8g2buv9Bz65H5
    ]

    inherit [
        BaseToken:   emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS,
        TokenHelper: bJKaNA2dLGxJEwp3xSok8g2buv9Bz65H5
    ]

    abstract PayableHACD(from_addr: address, dianum: u32, diamonds: bytes) {
        return 1
    }

    function external transfer_to(addr: address, amt: u64) -> u32 {
        return this.do_transfer(addr, addr, amt)
    }
}
```

### 3.2 顶层元素

| 元素 | 用途 |
|------|------|
| `deploy { ... }` | 部署配置（protocol_cost、nonce、construct_argv） |
| `library [ ... ]` | 外部合约的 `Name: Address` 对 |
| `inherit [ ... ]` | 继承链的 `Name: Address` 对 |
| `abstract Name(...) { ... }` | 系统支付钩子；返回 0 表示允许，非零表示拒绝 |

`pragma fitsh x.y.z` 必须是源码第一条有效语句。当前编译器版本为 `1.0.0`：

- `x` 表示非兼容主版本；主版本不一致会被拒绝。
- `y` 表示兼容的重大功能版本；源码要求更高 minor 时，旧编译器会拒绝。
- `z` 表示等效修改/小优化版本；patch 不一致时可编译，但会给出提示。

### 3.3 函数声明

```fitsh
function [external] [ircode|bytecode] name(param1: type1, param2: type2) -> ret_type { body }
```

- `external`：标记该函数可被 `CALL`（`External`）路径调用
- `ircode`：编译为 IR
- `bytecode`：编译为原始字节码
- 省略时，函数体默认编译为字节码。
- 修饰符顺序固定：`external` 在前，然后最多出现一个 `ircode` 或 `bytecode`。
- `virtual`、`inner`、`view`、`pure`、`struct` 在 Fitsh 1.0.0 严格源码中保留但未支持。

可见性说明：
- `external` 是外部调用解析使用的运行时可见性标记。
- 若命名在实践中容易误导，可在后续版本引入更清晰的别名。

### 3.4 部署配置

`deploy { ... }` 是严格部署配置块，不是运行期 `map`。

```fitsh
deploy {
    protocol_cost: amount("1:248"),
    nonce: 1u32,
    construct_argv: 0xaabb2244
}
```

- `protocol_cost` 必须使用 `amount("...")`。
- `nonce` 必须在 `u32` 范围内，推荐写 `u32` 后缀。
- `construct_argv` 必须是 bytes 字面量（`0x...`、`0b...` 或 ASCII 引号字节串）。
- 字段名不能重复。
- 每个字段名后必须写冒号。

### 3.5 常量

顶层和函数体常量共享同一套字面量语法：

```fitsh
const NAME [: type] = literal
const LIMIT: u64 = 100
const ENABLED: bool = true
const OWNER: address = emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS
const TAG: bytes = 0xaabb
```

常量只接受编译期字面量：整数、bool、bytes、char 或 address；不接受任意表达式。

---

## 4. 关键字列表与说明

### 4.1 声明与赋值

| 关键字 | 用途 |
|--------|------|
| `var` | 可变局部变量；分配槽位 |
| `let` | 不可变局部变量 |
| `bind` | 宏绑定；无槽位；内联展开 |
| `const` | 编译时常量 |
| `param` | 参数解包到槽位 |
| `lib` | 库绑定 |

### 4.2 控制流

| 关键字 | 用途 |
|--------|------|
| `if` / `else` | 条件 |
| `while` | 循环 |
| `return` | 返回值 |
| `end` | 终止执行 |
| `abort` | 中止 |
| `throw` | 抛出错误 |
| `assert` | 断言 |

### 4.3 调试与日志

| 关键字 | 用途 |
|--------|------|
| `print` | 调试输出 |
| `log` | 日志事件（2..5 个参数） |

### 4.4 调用指令

| 关键字 | 用途 |
|--------|------|
| `call` | 调用外部合约 |
| `callthis` / `callself` / `callsuper` | 内部调用 |
| `callextview` / `callusepure` | 只读调用 |
| `codecall` | CodeCall（原地 splice；源码层 `end` 可省略） |
| `bytecode` | 原始字节码注入 |

### 4.5 类型与字面量关键字

| 关键字 | 用途 |
|--------|------|
| `as` | 类型转换 |
| `is` | 类型检查 |
| `nil` | Nil 字面量 |
| `list` | 列表字面量 |
| `map` | 映射字面量 |
| `true` / `false` | 布尔字面量 |
| `u8` .. `u128` | 整数类型 |
| `bytes` | 字节类型 |
| `address` | 地址类型 |

---

## 5. 语法与示例

### 5.1 字面量

```fitsh
123                    // 整数
0xABC123               // 十六进制字节
0b11110000             // 二进制字节（8*n 位）
"hello \"world\" \n"   // 字符串（字节）带转义
'A'                    // 字符字面量（字节）
nil                    // Nil
true                   // 布尔
false                  // 布尔
emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS  // 地址
```

### 5.2 数组与列表

```fitsh
[1, 2, 3]              // 数组字面量
[]                     // 空列表
list { 1 2 3 }         // list 关键字（空格分隔）
```

### 5.3 映射

```fitsh
map { "key": "value", 1: addr }
map { }                // 空映射
```

### 5.4 运算符

| 类别 | 运算符 |
|------|--------|
| 算术 | `+`、`-`、`*`、`/`、`%`、`**` |
| 位运算 | `<<`、`>>`、`&`、`|`、`^` |
| 比较 | `==`、`!=`、`<`、`<=`、`>`、`>=` |
| 逻辑 | `&&`、`||`、`!` |
| 连接 | `++` |

优先级（高到低）：`!` → `**` → `*`/`/`/`%` → `+`/`-` → `<<`/`>>` → `>=`/`<=`/`>`/`<` → `==`/`!=` → `&` → `^` → `|` → `&&` → `||` → `++`

### 5.5 复合赋值

```fitsh
x += 1
x -= 1
x *= 2
x /= 2
```

### 5.6 控制流

```fitsh
if x > 0 {
    print "positive"
} else if x < 0 {
    print "negative"
} else {
    print "zero"
}

while cnt > 0 {
    cnt -= 1
}
```

### 5.7 块表达式

块 `{ stmt; stmt; value }` 执行语句并返回最后一个表达式：

```fitsh
var result = {
    var inner = 10
    inner + 1
}
// result == 11
```

---

## 6. 特殊语言结构

### 6.1 `param { ... }`

将列表参数解包到局部槽位 0、1、2、...

```fitsh
param { owner amount fee }
// owner -> 槽位 0, amount -> 槽位 1, fee -> 槽位 2
```

- 必须出现在函数体开头
- 规范 IR：`UNPACK(ROLL0, P0)`

### 6.2 `codecall lib_idx.func_sig`

- 支持可选参数列表：`codecall C.f`、`codecall C.f()`、`codecall C.f(nil)`、`codecall C.f(a)`、`codecall C.f(a, b)`
- `codecall C.f`、`codecall C.f()`、`codecall C.f(nil)` 三者等价
- 源码层尾随 `end` 可省略；`codecall C.f` 与 `codecall C.f` 后再写 `end` 都合法
- 用于底层委托

```fitsh
codecall 0.0xabcdef01
end
```

### 6.3 `bytecode { ... }`

按名称或编号注入原始字节码操作码：

```fitsh
bytecode { POP DUP SWAP }
```

### 6.4 `list { ... }`

列表的另一种写法（空格分隔）：

```fitsh
list { 1 2 3 }
```

### 6.5 `map { ... }`

使用 `:` 分隔的键值对：

```fitsh
map { "k": "v", 1: addr }
```

### 6.6 `log { ... }`

记录 2..5 个参数的事件。支持 `()`、`{}`、`[]` 分隔符：

```fitsh
log(1, 2)
log[1, 2, 3, 4, 5]
```

---

## 7. 隐式与显式类型转换

### 7.1 隐式转换

| 上下文 | 允许 | 主要拒绝 |
|--------|------|----------|
| 算术 | 整数拓宽；Bytes→Uint（trim 后 1..16 字节） | 空字节；bytes >16；Bool/Address/Nil |
| 字节操作（连接、切片） | Bool/Uint/Address→Bytes | Nil |
| 分支 | 真值（任意值） | — |
| 调用参数 | 整数拓宽；Bytes↔Address | Bytes→Uint，Bool→任意 |

### 7.2 显式转换

```fitsh
x as u8
x as u16
x as u32
x as u64
x as u128
x as bytes
x as address
```

### 7.3 类型检查

```fitsh
x is nil
x is not nil
x is list
x is map
x is u64
x is bytes
x is address
```

### 7.4 注意事项

1. **栈算术仅 uint**：`ADD`/`SUB`/`MUL` 等要求操作数已是 `u8`..`u128`；`Nil`/`Bool`/`Bytes` 不会自动进算术，需先 `as u64` 等显式 cast。
2. **比较不允许跨类型归一化**：`Bytes([0x01]) == U8(1)` 会失败；需显式转换后再比。
3. **显式 cast 宽于栈算术**：`nil as u64`、`1 as bool` 等在栈上可用；函数形参边界更严（仅 uint 族互转，见 `value-cast.md` §6.2）。
4. **Bytes↔Uint 不对称**：Uint→Bytes 是固定宽度；Bytes→Uint 使用 trim + 可变宽度（仅显式 `CU*`/`CTO U*`）。
5. **空字节**：`Bytes([])` 不能当零参与算术；需要数值零时用 `0 as u64`。
6. **Map 重复键**：字面量 `map { ... }` / `PACKMAP` 与 `MERGE` 拒重键（不可恢复失败 / **Fault**）；`INSERT` 对已有键覆盖（见 `value-cast.md` §8）。

### 7.5 操作数栈与手写 IR 参数顺序

Fitsh IR codegen 按子节点**从左到右**压栈：`subx` 在**栈底**，最后一个 child 在**栈顶**。这与源码参数顺序一致（`choose(cond, yes, no)` → 栈 `[cond, yes, no]`），而不是「条件在栈顶」的三元布局。

手写字节码 / `bytecode { … }` / 直接 IR 调用须遵守相同顺序。完整表格见 **`operand-stack.md`**。

易错点摘要：

| 主题 | 正确顺序（底 → 顶） | 常见误用 |
|------|---------------------|----------|
| `PACKLIST` / `PACKMAP` / `PACKTUPLE` | 元素…，**count 在最后** | 先压 count 的 `pack(n, …)` |
| `local_x_put` / `PUTX` | **idx, val** | `(val, idx)` |
| `local_x` / `GETX` | 仅 **idx** | |
| `XLG` | 栈上仅 rhs；lhs 为 `local[idx]` | 用 `XLG` 表达 `expr > local`（应 `GET` + 比较） |
| `XOP` | 栈上仅 rhs；语义 `local[idx] op=` | 当成双栈操作数 |
| `byte` / `BYTE` | **buf, idx** | `(idx, buf)` 或 `len, ost, buf` 式逆序 |
| `buf_cut` / `CUT` | **buf, start, len** | 误写为 `len, start, buf`（旧注释遗留） |
| `GGET` / `SLOAD` / … | key 原位被 value **替换** | 当成 `push get(key)` |

Fitsh 源码对字面量、`param`、槽位 `+=` 会生成正确形态；低层 opcode 主要影响 **`bytecode` 块**与 **`local_x_put` / `local_x` IR 调用**。

---

## 8. 变量与局部栈槽位

### 8.1 变量类型

| 声明 | 槽位 | 可变性 | 求值 |
|------|------|--------|------|
| `var` | 分配 | 可变 | 立即 |
| `let` | 分配 | 不可变 | 立即 |
| `bind` | 无槽位 | — | 惰性（引用时） |

### 8.2 槽位寻址

- `$0`、`$1`、... 直接引用槽位
- `var x $5 = 10` 显式写入槽位 5
- `param { a b c }` 将 a→0、b→1、c→2

### 8.3 直接槽位引用（`$0`、`$1`、...）

以 `$` 开头、后跟十进制数（0–255）的标识符是**直接槽位引用**。它们不经过符号表，直接绑定到指定索引的局部槽位。

#### 语法

| 形式 | 含义 |
|------|------|
| `$N` | 读取槽位 N（0 ≤ N ≤ 255） |
| `$N = expr` | 直接写入槽位 N |
| `var name $N = expr` | 将名称绑定到槽位 N 并赋值 |

#### 与 `param` 的关系

`param { a b c }` 会将参数解包到槽位 0、1、2。之后：

- `$0` 等同于第一个参数
- `$1` 等同于第二个参数
- `$2` 等同于第三个参数

可以通过 `$N` 读写，而无需使用参数名。

#### 读取与写入

```fitsh
param { owner amount }
$0 = "new owner"       // 写入槽位 0（覆盖 owner）
let first = $0         // 从槽位 0 读取
$4 = 999               // 写入槽位 4（若已分配）
```

#### 在 `var` / `let` 中显式指定槽位

将槽位绑定到名称：

```fitsh
var opt $10 = 123      // 将 "opt" 绑定到槽位 10，赋值为 123
let first_arg = $0     // 将槽位 0 读取到新绑定
```

#### 槽位冲突

- 同一槽位只能被带显式 `$N` 的 `var` 或 `let` 绑定一次
- 避免混用 `$N` 写入与对同一槽位的其他绑定
- 手工槽位通过 `reserve_slot` 预留；重复使用会触发 `slot N already bound`

#### 使用场景

| 场景 | 示例 |
|------|------|
| 覆盖参数 | `$0 = "new owner"` |
| 底层槽位访问 | 用 `$N` 精细控制 |
| 与 `unpack` 配合 | 先 `unpack(...)`，再使用 `$2`、`$3` 等 |
| 调试 / 检查 | 按索引查看槽位内容 |

#### 注意

- 直接写入会绕过常规检查（如 `let` 的不可变性）
- `$N` 可能覆盖参数或其他局部变量，需谨慎使用
- 槽位需先分配（如通过 `param` 或带显式 `$N` 的 `var`/`let`）才能使用

### 8.4 示例

```fitsh
param { owner amount }
var total = 200        // 自动分配槽位
var opt $10 = 123      // 显式槽位 10
$0 = "new owner"       // 写入槽位 0 (owner)
let first = $0         // 从槽位 0 读取
```

---

## 9. 合约资源空间

合约可使用六种资源空间。理解其作用域、生命周期和适用场景，有助于正确选择资源。

### 9.1 概览

| 资源 | 作用域 | 生命周期 | 键/索引 | 最大容量 | Fitsh API |
|------|--------|----------|---------|----------|-----------|
| **Locals** | 每次函数调用 | 调用期间 | 槽位索引 (0–255) | 256 槽位 | `var`、`let`、`param`、`$N` |
| **Heap** | 每次函数调用 | 调用期间 | 字节偏移 | 64 段 × 256 B | `heap_grow`、`heap_write`、`heap_read` |
| **Memory** | 每个合约 | 交易内 | 键（bytes） | ~16 键 | `memory_put`、`memory_get` |
| **Global** | 交易内全局 | 交易内 | 键（bytes） | ~20 键 | `global_put`、`global_get` |
| **Storage** | 每个合约 | 持久（需租金） | 键（bytes） | 租金决定 | `storage_load`、`storage_save`、`storage_del`、`storage_rest`、`storage_rent` |
| **Log** | 每个合约 | 持久（上链） | — | 每条 2–5 参数 | `log(...)` |

### 9.2 Locals（局部栈槽位）

**作用域**：当前函数调用（帧）。  
**生命周期**：到函数返回为止；随后回收。

**适用场景**：
- 局部变量（`var`、`let`、`param`）
- 中间临时值
- 参数与返回值暂存

**特点**：
- 按槽位索引（0–255）
- 访问快，无需哈希
- 返回时自动回收

**示例**：`param { a b }` → a 在槽位 0，b 在槽位 1；`var x = 1` → 自动分配槽位。

### 9.3 Heap（堆）

**作用域**：当前函数调用（帧）。  
**生命周期**：到函数返回为止。

**适用场景**：
- 单次调用内的大块二进制数据
- 按偏移访问（类似 C 数组）
- 通过 `heap_read` 读成 `Bytes` 后向被调用方传递原始字节

**特点**：
- 字节数组；按段扩展（每段 256 字节）
- `heap_grow(n)` 分配；`heap_write(offset, data)` / `heap_read(offset, len)` 访问
- `heap_write(offset, data)` 接受运行时 `u32` 偏移；`heap_write_x` / `heap_write_xl` 是 `u8` / `u16` 的立即数偏移辅助形式
- `heap_read_uint`、`heap_read_uint_long` 用于固定宽度整数读取
- 最多 64 段（约 16 KB）

**示例**：在单次调用内解析或构建二进制结构。

### 9.4 Memory（合约临时存储）

**作用域**：每个合约（`state_addr`）。  
**生命周期**：仅当前交易；交易结束后清空。

**适用场景**：
- 同一交易内多次调用同一合约时传递数据
- 多步骤流程（如 充值 → 兑换 → 提现）
- 交易内跨调用的中间状态

**特点**：
- 键值对（键为 bytes）
- 每个合约有独立内存
- 不持久；仅在交易内有效

**示例**（AMM）：`prepare` 保存 `in_sat`、`in_zhu`；`PayableSAT` / `PayableHAC` 读取并完成流程。

```fitsh
// 步骤 1：prepare
memory_put("in_sat", sat)
memory_put("in_zhu", zhu)

// 步骤 2：PayableHAC（同一交易内后续调用）
var in_zhu = memory_get("in_zhu")
memory_put("in_sat", nil)  // 使用后清空
```

### 9.5 Global（交易全局临时）

**作用域**：整个交易。  
**生命周期**：仅当前交易。

**适用场景**：
- 同一交易内多合约共享数据
- 交易级标志或计数器
- 跨合约协同

**特点**：
- 交易内唯一键值映射
- 所有合约共享同一映射
- 不持久

**示例**：多合约共用的交易级 step 或 id。

```fitsh
global_put("tx_step", 1)
// ... 同一交易内，另一合约
var step = global_get("tx_step")
```

### 9.6 Storage（合约状态）

**作用域**：每个合约（`state_addr`）。  
**生命周期**：持久；需支付租金；跨区块保留。

**适用场景**：
- 持久状态（余额、配置、总量等）
- 需跨交易和区块保留的数据

**特点**：
- 键值对；键为 bytes（如 `"b_" ++ addr`）
- 按租金管理；`storage_rent(key, amount)` 支付
- `storage_rest(key)` 查询到期
- 值类型：Nil、Bool、Uint、Address、Bytes
- 单值最大约 1280 字节
- 1 个租期 = 100 个区块；单条目最大租期 = 30000 个租期
- 边界语义：在到期高度当块数据仍有效，`storage_rest(key)` 返回 `0`；下一块才算过期

**示例**：代币余额、AMM 储备、配置。

```fitsh
bind bk = "b_" ++ addr
var balance = storage_load(bk)
if balance is nil {
    balance = 0 as u64
}
storage_save(bk, balance + amount)
```

### 9.7 Log（事件）

**作用域**：每个合约（`state_addr`）。  
**生命周期**：持久（链上事件）。

**适用场景**：
- 向索引器或前端发出事件
- 操作审计记录

**特点**：
- 每条 2–5 个参数
- 上链存储；合约内不可查询

**示例**：`log("Transfer", from, to, amount)`

### 9.8 选择指南

| 需求 | 资源 |
|------|------|
| 局部变量 / 参数 | Locals |
| 单次调用内大块二进制 | Heap |
| 同一合约多次调用间传参 | Memory |
| 同一交易内多合约共享 | Global |
| 持久合约状态 | Storage |
| 发出事件 | Log |

### 9.9 小结

- **Locals / Heap**：调用级；用于单次调用内计算。
- **Memory**：合约级，仅交易内；用于交易内多步骤流程。
- **Global**：交易级；用于跨合约协同。
- **Storage**：合约级，持久；用于长期状态。
- **Log**：合约级，持久；用于事件。

---

## 10. `bind` 宏绑定

### 10.1 行为

- **惰性求值**：声明时不求值；引用时才求值
- **无槽位**：不分配；无 `PUT`/`GET`
- **内联展开**：每次引用时复制表达式模板

### 10.2 使用场景

```fitsh
bind bk = "b_" ++ addr
var balance = storage_load(bk)
storage_save(bk, balance + 100)
```

### 10.3 注意

`bind` 表达式中的副作用（如 `storage_save`、`print`）只有在绑定被**读取**时才会执行。若从不读取，则不会发生。如有副作用请使用 `var` 立即执行。

---

## 11. 其他特色语法

### 11.1 函数调用语法

| 语法 | 操作码 | 用途 |
|------|--------|------|
| `lib.func(...)` | CALL | 状态变更调用 |
| `lib:func(...)` | CALLEXTVIEW | 视图调用 |
| `lib::func(...)` | CALLUSEPURE | 纯 code-local 调用 |
| `calluseview 1::sig(...)` | CALLUSEVIEW | view code-local 调用 |
| `this.func(...)` | CALLTHIS | 当前合约 |
| `self.func(...)` | CALLSELF | 当前合约 |
| `super.func(...)` | CALLSUPER | 继承链父级 |

库解析说明：
- `CALL`（`lib.func(...)`）先解析库地址，再只搜索目标合约及其直接父级。
- `CALLEXTVIEW` 在目标合约及其直接父级上解析，`CALLUSEVIEW/CALLUSEPURE/CODECALL` 仅停留在目标库精确根上。

### 11.2 调用权限与状态访问控制

VM 现在把运行时权限模型拆成 **ExecCtx = (ExecDomain, FrameMode)**。

- `ExecDomain` 描述当前调用策略来源：`TopMain`、`TopP2sh`、`TopAbst`、`Contract`。
- `FrameMode` 描述当前状态访问强度：`External`、`Inner`、`View`、`Pure`。
- 固定模式调用（`CALL`、`CALLEXTVIEW`、`CALLUSEVIEW`、`CALLUSEPURE`、`CALLTHIS`、`CALLSELF`、`CALLSUPER`）会切到 `ExecDomain::Contract`，并设置固定 `FrameMode`。
- `CODECALL` 会完整继承调用方的 `ExecCtx`，并在当前帧原地执行。

#### 常用 Call 指令分发表

| 调用 | 语法 | 被调用 `ExecCtx` | 查找起点 | 是否查继承链 | 帧行为 | 状态读取 | 状态写入 |
|------|------|------------------|----------|--------------|--------|----------|----------|
| `call` | `lib.func(...)` | `Contract + External` | 库目标合约 | 是（仅目标 + 直接父级） | 新建帧 | 允许 | 允许 |
| `callextview` | `lib:func(...)` | `Contract + View` | 库目标合约 | 是（仅目标 + 直接父级） | 新建帧 | 允许 | 禁止 |
| `calluseview` | `calluseview lib::sig(...)` | `Contract + View` | 库目标合约 | 否（仅精确根） | 新建帧 | 允许 | 禁止 |
| `callusepure` | `lib::func(...)` | `Contract + Pure` | 库目标合约 | 否（仅精确根） | 新建帧 | 禁止 | 禁止 |
| `codecall` | `codecall lib::sig` | 继承调用方 `ExecCtx` | 库目标合约 | 否（仅精确根） | 原地执行（不建帧） | 继承 | 继承 |
| `callthis` | `this.func(...)` | `Contract + Inner` | `state_addr` | 是 | 新建帧 | 允许 | 允许 |
| `callself` | `self.func(...)` | `Contract + Inner` | `code_owner` | 是 | 新建帧 | 允许 | 允许 |
| `callsuper` | `super.func(...)` | `Contract + Inner` | `code_owner` 的直接父级 | 入口集只取直接父级，不再额外 DFS | 新建帧 | 允许 | 允许 |

#### 地址迁移规则（`state_addr` / `code_owner`）

| 调用 | `state_addr` | `code_owner` |
|------|--------------|--------------|
| `call` | 切换为库目标合约 | 切换为实际命中函数的所有者（目标本身或直接父级） |
| `callextview` | 不变 | 切换为实际命中函数的所有者（目标本身或直接父级） |
| `calluseview` | 不变 | 切换为精确库根上命中的函数所有者 |
| `callusepure` | 不变 | 切换为精确库根上命中的函数所有者 |
| `codecall` | 不变 | 当前帧切换为库目标合约 |
| `callthis` | 不变 | 从 `state_addr` 链命中的所有者 |
| `callself` | 不变 | 从 `code_owner` 链命中的所有者 |
| `callsuper` | 不变 | 从直接父级入口集命中的所有者 |

**状态** = 存储、全局、内存、日志。

**重要说明**：`codecall` 在**当前帧**中执行并**完整继承调用方的 ExecCtx** —— 它**没有独立的状态访问控制逻辑**。codecall 体内的所有状态操作（存储读写、EXTACTION/EXTENV/EXTVIEW、NTFUNC/NTENV/NTCTL）都受继承下来的 domain/frame 权限限制。与隔离的新调用帧不同，`codecall` **不会**额外禁止后续嵌套调用；splice 代码中的 `CALL*` 仍按普通 `(domain, frame, depth)` 运行时门控执行。

#### 正交权限矩阵

Domain 轴限制：

| `ExecDomain` | 允许的调用 |
|---|---|
| `TopMain` | `CALL`、`CALLEXTVIEW`、`CALLUSEVIEW`、`CALLUSEPURE`、`CODECALL` |
| `TopP2sh` | `CALLEXTVIEW`、`CALLUSEVIEW`、`CALLUSEPURE`、`CODECALL` |
| `TopAbst` | `CALLTHIS`、`CALLSELF`、`CALLSUPER`、`CALLEXTVIEW`、`CALLUSEVIEW`、`CALLUSEPURE`、`CODECALL` |
| `Contract` | 无额外 domain 限制 |

Frame 轴限制：

| `FrameMode` | 允许的调用 | 状态读取 | 状态写入 |
|---|---|---|---|
| `External` | 不受 frame 轴额外限制 | 允许 | 允许 |
| `Inner` | 不受 frame 轴额外限制 | 允许 | 允许 |
| `View` | `CALLEXTVIEW`、`CALLUSEVIEW`、`CALLUSEPURE` | 允许 | 禁止 |
| `Pure` | 仅 `CALLUSEPURE` | 禁止 | 禁止 |

`TopAbst` 禁止 `CALL`（External），用于防止支付钩子经由外部合约重入。`CODECALL` 本身不会额外增加“禁止嵌套调用”的限制；splice 代码中的嵌套调用仍受常规 entry/effect/depth 检查约束。

#### 各模式下的状态访问控制矩阵

**存储/全局/内存/日志访问**：

| 模式 | 存储读取 | 存储写入 | 全局/内存读取 | 全局/内存写入 | 日志 |
|------|----------|----------|---------------|---------------|------|
| Main、P2sh、Abst | 允许 | 允许 | 允许 | 允许 | 允许 |
| External、Inner | 允许 | 允许 | 允许 | 允许 | 允许 |
| View | 允许 | 禁止 | 允许 | 禁止 | 禁止 |
| Pure | 禁止 | 禁止 | 禁止 | 禁止 | 禁止 |

**外部调用（EXTACTION / EXTENV / EXTVIEW）**：

| 模式 | EXTACTION | EXTENV | EXTVIEW | 备注 |
|------|-----------|--------|---------|------|
| **Main**（depth==0） | ✅ 允许 | ✅ 允许 | ✅ 允许 | 完全访问 |
| **Main**（depth>0） | ❌ 禁止 | ✅ 允许 | ✅ 允许 | EXTACTION 在嵌套调用中被阻止，包括嵌套 codecall 路径 |
| **P2sh、Abst** | ❌ 禁止 | ✅ 允许 | ✅ 允许 | EXTACTION 仅限入口层 |
| **External、Inner** | ❌ 禁止 | ✅ 允许 | ✅ 允许 | EXTACTION 仅限入口层 |
| **View** | ❌ 禁止 | ✅ 允许 | ✅ 允许 | 只读环境访问 |
| **Pure** | ❌ 禁止 | ❌ 禁止 | ❌ 禁止 | 无外部状态访问 |

**原生调用（NTFUNC / NTENV / NTCTL）**：

| 操作码 | 原生调用 | 源码参数数 | Pure 模式 | View 模式 | Edit（Main/External/Inner） | 功能 |
|--------|----------|------------|-----------|-----------|------------------------------|------|
| NTENV | `context_address` | 0 | ❌ 禁止（`InstDisabled`） | ✅ 允许 | ✅ 允许 | 读取 VM 执行上下文地址 |
| NTFUNC | `sha2/sha3/ripemd160` | 1 | ✅ 允许 | ✅ | ✅ | 纯哈希函数 |
| NTFUNC | `hac_to_mei/zhu(amount)`、`mei/zhu_to_hac(count)`、`fold64_to_u64(data)`、`u64_to_fold64(n)` | 1 | ✅ 允许 | ✅ | ✅ | 纯金额/编码转换（解码/编码入参类型不同，见 §11.4） |
| NTFUNC | `pack_asset(serial, amount)` | 2 | ✅ 允许 | ✅ | ✅ | 由两个 u64 组装 AssetAmt bytes |
| NTFUNC | `address_ptr` | 1 | ✅ 允许 | ✅ | ✅ | 纯地址指针提取 |
| NTCTL | `defer`、`intent_*` 写（如 `intent_put`、`intent_pop`） | 0–2 | ❌ 禁止（`IntentError`） | ❌ 禁止（`IntentError`） | ✅ 允许（仅 Edit） | 修改 tx 本地 VM 状态（defer 注册表、intent 作用域） |
| NTCTL | `intent_*` 读（如 `intent_get`、`intent_kind`、`intent_len`） | 0–2 | ❌ 禁止（`IntentError`） | ✅ 允许 | ✅ 允许 | 读取 tx 本地 intent 状态 |

**Native 栈约定**（操作码级 vs 源码级参数个数）：

VM 使用两类 native 调用栈模型，这是刻意设计，不要假设所有 native 共享同一栈形。

| 模型 | 操作码 | 操作码栈 | 源码零参形式 |
|------|--------|----------|--------------|
| 环境读 | `NTENV`、`ACTENV` | `0 → 1`（不消耗操作数） | `context_address()`、`block_height()` |
| 带参调用 | `NTFUNC`、`NTCTL` | `1 → 1`（固定弹栈 1 个 argv） | 源码无参时编译器先压入 `nil` |

**`NTCTL`** / **`NTFUNC`** 的操作码 metadata 始终要求 1 个栈输入。注册表中 `argv_len = 0` 的 native（如 `intent_pop()`、`intent_kind()`）仍走带参调用模型：编译器生成 `PNIL; NTCTL idx`，运行期仅接受 `nil` 作为 argv。手写字节码须同样保留 `nil` 占位。反编译器在 `hide_default_call_argv` 开启时可能隐藏该占位（仅影响显示）。详见 `call-standard.md` §11.3。

**`NTENV`** 的操作码 metadata 为真零输入（`0 → 1`），不使用 `nil` 占位。

**Effect 门控错误码**（为何禁止时并非同一错误码）：

| 层级 | 示例 | Pure 禁止 | 典型错误 |
|------|------|-----------|----------|
| VM 操作码读门控 | `SGET`、`NTENV` | 是 | `InstDisabled` — 当前 effect 下指令禁用 |
| Host action 门控 | `ACTENV`、`ACTVIEW`、`EXTENV`、`EXTVIEW` | 是 | `ActDisabled` — 当前 effect 下 action/env/view 调用被阻止 |
| Native ctl 门控 | `NTCTL` `intent_*` | 按 native（`ctl_require_edit` / `ctl_require_non_pure`） | `IntentError` — 含 native 名；View 可读而写仍被禁 |

`NTENV` 使用 `InstDisabled`（而非 `ActDisabled`），因其在操作码层读取 VM 本地执行状态，与存储读指令（`nsr!` → `InstDisabled`）同类。`ACTENV` 经 host action 层，在 `Pure` 下因此报告 `ActDisabled`。

**小结**：
- **EXTACTION**（资产转移）：仅 `Main` 模式在 `depth == 0`
- **EXTENV**（`block_height`、`tx_main_address`）：在 `Pure` 中禁止，其他允许（`ActDisabled`）
- **EXTVIEW**（`check_signature`、`balance`）：在 `Pure` 中禁止，其他允许 —— 只读链状态查询（`ActDisabled`）
- **NTFUNC**（纯计算）：所有模式均允许，包括 `Pure`
- **NTENV**（`context_address`）：在 `Pure` 中禁止（读取 VM 状态，`InstDisabled`），其他允许
- **NTCTL**（`defer`、`intent_*`）：写操作仅 Edit；读操作 View 允许、Pure 禁止；源码零参在字节码层须 `nil` 栈占位

#### EXTACTION 限制

| 条件 | EXTACTION 是否允许 |
|------|-------------------|
| mode == Main 且 depth == 0 | 允许 |
| mode != Main 或 depth > 0 | 禁止 |

`transfer_hac_to`、`transfer_sat_to` 等只能在顶层主调用中执行。在 `codecall`、抽象/支付钩子及嵌套调用中均禁用。

#### 小结

- **call** → External：完全状态访问；被调用方须标记为 `external`
- **callextview** → View：只读；禁止存储/全局/内存/日志写入
- **callusepure** → Pure：无状态访问；仅纯计算及嵌套 CALLUSEPURE
- **codecall** → 继承当前模式；原地执行；允许继续嵌套调用，但仍受常规运行时门控约束；EXTACTION 仍取决于继承到的 mode/depth
- **callthis/callself/callsuper** → Inner：完全状态访问；仅内部调用

### 11.3 函数查找：`this`、`self` 与 `super`

VM 在执行过程中维护两个关键地址：

- **state_addr**：存储/日志的所有者 —— 最初被调用的合约（入口）。在嵌套 inner/view/pure/codecall 调度中保持不变。
- **code_owner**：代码所有者 —— 当前正在执行其代码的合约。当解析到的调用命中其他所有者时会变化。

| 调用 | 解析范围 | 查找顺序 |
|------|----------|----------|
| `this.func(...)` | state_addr | 当前合约 → 直接父级（按顺序） |
| `self.func(...)` | code_owner | 当前合约 → 直接父级（按顺序） |
| `super.func(...)` | 仅 code_owner 的父级 | 仅直接父级（跳过当前 owner） |

**何时会不同？** 当 `super` 或 `self` 将执行转移到父级代码时：`code_owner` 变为父级，但 `state_addr` 仍为子级。此时 `this` 仍在子级（存储上下文）中解析，而 `self` 在父级（当前代码所有者）中解析。

#### 示例 1：直接调用（无继承）

```fitsh
contract Token {
    function external balance_of(addr: address) -> u64 {
        bind bk = "b_" ++ addr
        var balance = storage_load(bk)
        if balance is nil {
            balance = 0 as u64
        }
        return balance
    }
    function external transfer_to(addr: address, amt: u64) -> u32 {
        return this.do_transfer(addr, addr, amt)
    }
    function do_transfer(from: address, to: address, amt: u64) -> u32 {
        // ...
    }
}
```

此处 `this`、`self`、`super` 都在同一合约中解析。`this.do_transfer(...)` 与 `self.do_transfer(...)` 行为相同。

#### 示例 2：继承 —— `inherit` 链

```fitsh
contract Base {
    function get_value() -> u64 { return 3 }
}
contract Parent {
    inherit [Base: 0x...]
    function get_value() -> u64 { return 2 }
    function compute() -> u64 {
        return this.get_value() * 10000 + self.get_value() * 100 + super.get_value()
    }
}
contract Child {
    inherit [Parent: 0x...]
    function get_value() -> u64 { return 1 }
    function external run() -> u32 {
        let v = super.compute()
        assert v == 10203
        return 0
    }
}
```

- `Child.run()` 调用 `super.compute()` → 在 Parent 中解析（跳过 Child）。执行 Parent 的 `compute()`。
- **state_addr** = Child（不变）
- **code_owner** = Parent（当前代码所有者）

在 Parent 的 `compute()` 内：

- `this.get_value()` → 在 **state_addr**（Child）中解析 → Child 的 `get_value()` → **1**
- `self.get_value()` → 在 **code_owner**（Parent）中解析 → Parent 的 `get_value()` → **2**
- `super.get_value()` → 跳过 Parent，在 Parent 的继承中查找 → Base 的 `get_value()` → **3**

结果：`1*10000 + 2*100 + 3 = 10203`

#### 示例 3：继承顺序（先匹配优先）

```fitsh
contract A { function f() -> u64 { return 10 } }
contract B { function f() -> u64 { return 20 } }
contract C {
    inherit [A: 0x..., B: 0x...]
    function external run() -> u64 {
        return self.f()
    }
}
```

`self.f()` 按 C → A → B 顺序查找。A 先定义 `f`，因此结果为 **10**。继承顺序决定优先级。

#### 示例 4：`super` 跳过当前合约

```fitsh
contract Base {
    function helper() -> u64 { return 100 }
}
contract Child {
    inherit [Base: 0x...]
    function helper() -> u64 { return 1 }
    function external run() -> u64 {
        return super.helper()
    }
}
```

`super.helper()` 跳过 Child，仅在 Base 中查找。结果：**100**（Base 的实现）。

#### 示例 5：何时使用各自

| 用途 | 推荐 |
|------|------|
| 调用自身或继承函数，从存储上下文解析 | `this` |
| 从当前代码所有者调用（如 `super` 进入父级后） | `self` |
| 调用父级实现，绕过当前覆盖 | `super` |

**总结**：`this` = 存储上下文；`self` = 当前代码所有者；`super` = 仅父级链。

### 11.4 原生调用

内置 native 分三类操作码族，栈形与 effect 门控不同 —— 见 §11.2 权限矩阵与 `call-standard.md` §11.3。

| 族 | 操作码 | 示例 | 栈模型 |
|----|--------|------|--------|
| 纯函数 | `NTFUNC` | `sha2`、`hac_to_mei` | `1 → 1`；栈上始终 1 个 argv |
| VM 环境读 | `NTENV` | `context_address` | `0 → 1`；字节码级真零参 |
| 运行时控制 | `NTCTL` | `defer`、`intent_*` | `1 → 1`；源码零参时用 `nil` 占位 |

| 函数 | 说明 |
|------|------|
| `context_address()` | 当前执行上下文地址（`NTENV`；`Pure` 禁止，错误 `InstDisabled`） |
| `block_height()` | 当前区块高度 |
| `sha2(data)` | SHA-256 哈希 |
| `sha3(data)` | SHA3 哈希 |
| `ripemd160(data)` | RIPEMD-160 哈希 |
| `hac_to_mei(amount)` | 将 HAC **Amount 序列化字节**解码为 mei 数量（`u64`）；缓冲区须被完整解析，拒绝尾部多余字节 |
| `hac_to_zhu(amount)` | 将 HAC **Amount 序列化字节**解码为 zhu 数量（`u128`）；同上 |
| `mei_to_hac(mei)` | 将 **mei 单位数量**（整数标量，非 Amount 字节）编码为 HAC Amount 字节 |
| `zhu_to_hac(zhu)` | 将 **zhu 单位数量**（整数标量，非 Amount 字节）编码为 HAC Amount 字节 |
| `u64_to_fold64(n)` | 将 u64 整数标量编码为 Fold64 bytes |
| `fold64_to_u64(data)` | 将 Fold64 序列化字节解码为 u64；缓冲区须被完整解析 |

`hac_to_*` 与 `mei/zhu_to_hac` 的入参类型有意不对称：前者接链上 Amount 线格式（常与 `transfer_*` 的 `amount` 或 `buf_*` 切片后的字节连用），后者接单位计数整数。典型往返：`mei_to_hac(n)` → Amount bytes → `hac_to_mei(...)` → `n`。勿把整数传给 `hac_to_*`，也不要把 Amount 字节传给 `mei/zhu_to_hac`。
| `pack_asset(serial, amount)` | 将 `(u64,u64)` 编码为 AssetAmt bytes |

### 11.5 扩展动作（EXTACTION）

| 函数 | 说明 |
|------|------|
| `transfer_hac_to(addr, amount)` | 转账 HAC |
| `transfer_hac_from(addr, amount)` | 从地址转出 HAC |
| `transfer_hac_from_to(from, to, amount)` | 在地址间转账 HAC |
| `transfer_sat_to`、`transfer_sat_from`、`transfer_sat_from_to` | SAT 转账 |
| `transfer_hacd_single_to`、`transfer_hacd_to` 等 | HACD 转账 |
| `transfer_asset_to`、`transfer_asset_from`、`transfer_asset_from_to` | 资产转账 |

**注意**：`codecall` 上下文中禁用 EXTACTION。

### 11.6 存储函数

| 函数 | 说明 |
|------|------|
| `storage_load(key)` | 加载值 |
| `storage_save(key, value)` | 保存值 |
| `storage_del(key)` | 删除键 |
| `storage_rest(key)` | 获取租期到期 |
| `storage_rent(key, amount)` | 支付租金 |

### 11.7 内存与堆

| 函数 | 说明 |
|------|------|
| `memory_put(key, value)` | 写入内存；`memory_put(key, nil)` 会清空该 key。仅可作语句使用，不返回值。 |
| `memory_get(key)` | 从内存读取 |
| `global_put(key, value)` | 全局临时存储；`global_put(key, nil)` 会清空该 key。仅可作语句使用，不返回值。 |
| `global_get(key)` | 全局读取 |
| `heap_grow(n)` | 扩展堆 |

TEX 说明：
- `TexCellAct` 只能在交易顶层执行，不能从运行时 `CALL` 上下文触发。
- TEX 条件检查的是当前交易内、当前动作执行点的状态，发生在最终 TEX 清算之前。
- TEX 的钻石 `get` 只声明数量；最终具体钻石名会在清算阶段按整笔交易的 FIFO 顺序分配。
| `heap_write(offset, data)` | 在运行时 `u32` 偏移处写入堆 |
| `heap_read(offset, len)` | 从堆读取 |

### 11.8 数据结构函数

| 函数 | 说明 |
|------|------|
| `length(list)` | 列表长度 |
| `keys(map)` | 映射键 |
| `values(map)` | 映射值 |
| `has_key(map, key)` | 检查键 |
| `take_first(list)` | 取首元素；消费列表，其余元素丢弃 |
| `take_last(list)` | 取末元素；消费列表，其余元素丢弃 |
| `append(list, item)` | 追加 |
| `insert(list, index, item)` | 插入 |
| `remove(list, index)` | 移除 |
| `clone(val)` | 克隆 |
| `clear(collection)` | 清空 |

### 11.9 缓冲区函数

| 函数 | 说明 |
|------|------|
| `buf_cut(buf, start, len)` | 切片 |
| `buf_left(n, buf)` | 左侧 n 字节 |
| `buf_right(n, buf)` | 右侧 n 字节 |
| `buf_left_drop(n, buf)` | 丢弃左侧 n 字节 |
| `buf_right_drop(n, buf)` | 丢弃右侧 n 字节 |
| `byte(buf, index)` | 索引处字节 |
| `size(buf)` | 大小 |

### 11.10 其他扩展函数

| 函数 | 说明 |
|------|------|
| `check_signature(addr)` | 验证签名 |
| `balance(addr)` | 余额字节 |

### 11.11 特别注意：可选尾逗号/分号

Fitsh 允许在**语句和表达式末尾省略逗号或分号**。这是与多数类 C 语言不同的语法特征。

- **语句**：`var x = 1` 与 `var x = 1;` 等价
- **表达式序列**：`list { 1 2 3 }` 中元素用空格分隔，末尾无需逗号
- **顶层元素**：`library`、`inherit` 等数组项之间逗号可选

示例：

```fitsh
var a = 1
var b = 2
list { 1 2 3 }
map { "k": "v" }
```

从 Solidity 等语言转来的开发者需注意：Fitsh 不强制要求行尾分号。

---

## 快速参考

- **函数参数上限**：15（pack list）；更多时用 `list`/`map` 包装
- **函数签名**：仅按名称 4 字节哈希；无重载
- **`param`**：必须位于函数体开头
- **`codecall`**：源码层尾随 `end` 可省略；原地 splice 执行
- **`bind`**：惰性；有副作用时用 `var`
