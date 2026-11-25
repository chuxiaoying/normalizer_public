//! Java 原生 primitive 判定 + 泛型擦除。
//!
//! ## 用户教导（20260428143804）
//!
//! "is_normalization_primitive 这个不应该存在。你最多是有一个 java type to math primitive
//! type 的 mapping，比如将 java.lang.String 映射为原生类型枚举 String，将 Double/Integer/Long
//! 映射为 number。这是个 mapping，而不是个白名单。"
//!
//! 整改：`is_normalization_primitive` 白名单已删除。需要判"是不是 primitive"的调用点
//! 应改用 `Primitive::from_java_fqn(ty).is_some()` —— java type → math primitive 的 mapping。
//!
//! 本模块保留：
//! - `is_java_primitive`：Java 原生 primitive 关键字判定（int/long/boolean 等 JLS 保留字）
//! - `is_any_primitive`：`is_java_primitive || Primitive::from_java_fqn.is_some()`
//! - `erase_generic`：泛型擦除（`List<X>` → `List`）

use crate::primitive_enum::Primitive;

/// 判断是否为 Java 原生 primitive 类型关键字（int/long/boolean 等 JLS 保留字）。
///
/// 这些类型走 `JavaType::Primitive` 分支。P0 架构断言：SymVar primitive_type 允许 Java 原生 primitive。
pub fn is_java_primitive(ty: &str) -> bool {
    matches!(
        ty,
        "int" | "long" | "boolean" | "double" | "float" | "byte" | "short" | "char" | "void"
    )
}

/// 判断是否为任意 primitive（Java 原生关键字 + math::Primitive 可映射的 Java 类）。
///
/// 用户 20260428143804 教导：改用 Primitive::from_java_fqn mapping，不是白名单。
///
/// P0 架构断言：SymVar primitive_type 必须通过此检查。
/// 业务 Pojo 类型应用 HeapRef 表达，不能用 SymVar。
pub fn is_any_primitive(ty: &str) -> bool {
    matches!(ty, "integer" | crate::domains::decimal::VALUE_TAG)
        || is_java_primitive(ty) || Primitive::from_java_fqn(ty).is_some()
}

/// 擦除 Java 泛型参数，返回 raw type FQN。
///
/// Java 泛型在 JVM 层 erasure，`List<ProfileItem>` 的 raw type 是 `java.util.List`。
/// 归一化不关心泛型参数（类型参数在 sym_exec 阶段已被实例化展开），
/// type_registry / method_registry 等以 raw type FQN 作键。
///
/// 规则：找第一个 `<`，截断前缀，trim 空白。
pub fn erase_generic(type_qn: &str) -> String {
    if let Some(idx) = type_qn.find('<') {
        type_qn[..idx].trim().to_string()
    } else {
        type_qn.to_string()
    }
}
