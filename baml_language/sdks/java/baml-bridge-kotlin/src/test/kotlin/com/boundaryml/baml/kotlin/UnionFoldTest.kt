package com.boundaryml.baml.kotlin

import baml_bridge.Union2
import baml_bridge.Union3
import baml_bridge.Union10
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

/**
 * `fold` / `armNOrNull` over the runtime union arity family. Uses concrete arm
 * records (pure Java value classes — no engine), so the whole suite runs offline.
 */
class UnionFoldTest {

    @Test
    fun fold_selects_the_present_arm() {
        val a: Union2<String, Int> = Union2.Arm0("hi")
        assertEquals("s:hi", a.fold({ "s:$it" }, { "i:$it" }))

        val b: Union2<String, Int> = Union2.Arm1(7)
        assertEquals("i:7", b.fold({ "s:$it" }, { "i:$it" }))
    }

    @Test
    fun fold_is_exhaustive_by_signature_across_arities() {
        // One lambda per arm; the arm actually present drives the result.
        val u3: Union3<String, Int, Boolean> = Union3.Arm2(true)
        assertEquals("b:true", u3.fold({ "s:$it" }, { "i:$it" }, { "b:$it" }))

        // Highest arity compiles and folds too (last arm).
        val u10: Union10<Int, Int, Int, Int, Int, Int, Int, Int, Int, String> =
            Union10.Arm9("tenth")
        val r = u10.fold(
            { "0" }, { "1" }, { "2" }, { "3" }, { "4" },
            { "5" }, { "6" }, { "7" }, { "8" }, { "n:$it" },
        )
        assertEquals("n:tenth", r)
    }

    @Test
    fun armOrNull_narrows_to_the_present_arm() {
        val a: Union2<String, Int> = Union2.Arm0("hi")
        assertEquals("hi", a.arm0OrNull())
        assertNull(a.arm1OrNull())

        val b: Union2<String, Int> = Union2.Arm1(7)
        assertNull(b.arm0OrNull())
        assertEquals(7, b.arm1OrNull())
    }
}
