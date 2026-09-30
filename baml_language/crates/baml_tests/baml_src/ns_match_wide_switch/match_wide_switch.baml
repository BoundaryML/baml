// A switch has no widest form: whatever its arms, a value reaches its arm
// through one table. These matches are wider than a hash is searched for, so
// their tables hold the keys in order.
//
// The class arms are bare type patterns: an arm that binds (`let w: Wide00`)
// tests and binds at once, and takes no switch.

// ============================================================================
// Seventy class arms
// ============================================================================

class Wide00 { weight int }
class Wide01 { weight int }
class Wide02 { weight int }
class Wide03 { weight int }
class Wide04 { weight int }
class Wide05 { weight int }
class Wide06 { weight int }
class Wide07 { weight int }
class Wide08 { weight int }
class Wide09 { weight int }
class Wide10 { weight int }
class Wide11 { weight int }
class Wide12 { weight int }
class Wide13 { weight int }
class Wide14 { weight int }
class Wide15 { weight int }
class Wide16 { weight int }
class Wide17 { weight int }
class Wide18 { weight int }
class Wide19 { weight int }
class Wide20 { weight int }
class Wide21 { weight int }
class Wide22 { weight int }
class Wide23 { weight int }
class Wide24 { weight int }
class Wide25 { weight int }
class Wide26 { weight int }
class Wide27 { weight int }
class Wide28 { weight int }
class Wide29 { weight int }
class Wide30 { weight int }
class Wide31 { weight int }
class Wide32 { weight int }
class Wide33 { weight int }
class Wide34 { weight int }
class Wide35 { weight int }
class Wide36 { weight int }
class Wide37 { weight int }
class Wide38 { weight int }
class Wide39 { weight int }
class Wide40 { weight int }
class Wide41 { weight int }
class Wide42 { weight int }
class Wide43 { weight int }
class Wide44 { weight int }
class Wide45 { weight int }
class Wide46 { weight int }
class Wide47 { weight int }
class Wide48 { weight int }
class Wide49 { weight int }
class Wide50 { weight int }
class Wide51 { weight int }
class Wide52 { weight int }
class Wide53 { weight int }
class Wide54 { weight int }
class Wide55 { weight int }
class Wide56 { weight int }
class Wide57 { weight int }
class Wide58 { weight int }
class Wide59 { weight int }
class Wide60 { weight int }
class Wide61 { weight int }
class Wide62 { weight int }
class Wide63 { weight int }
class Wide64 { weight int }
class Wide65 { weight int }
class Wide66 { weight int }
class Wide67 { weight int }
class Wide68 { weight int }
class Wide69 { weight int }

type AnyWide = Wide00 | Wide01 | Wide02 | Wide03 | Wide04 | Wide05 | Wide06 | Wide07 | Wide08 | Wide09 | Wide10 | Wide11 | Wide12 | Wide13 | Wide14 | Wide15 | Wide16 | Wide17 | Wide18 | Wide19 | Wide20 | Wide21 | Wide22 | Wide23 | Wide24 | Wide25 | Wide26 | Wide27 | Wide28 | Wide29 | Wide30 | Wide31 | Wide32 | Wide33 | Wide34 | Wide35 | Wide36 | Wide37 | Wide38 | Wide39 | Wide40 | Wide41 | Wide42 | Wide43 | Wide44 | Wide45 | Wide46 | Wide47 | Wide48 | Wide49 | Wide50 | Wide51 | Wide52 | Wide53 | Wide54 | Wide55 | Wide56 | Wide57 | Wide58 | Wide59 | Wide60 | Wide61 | Wide62 | Wide63 | Wide64 | Wide65 | Wide66 | Wide67 | Wide68 | Wide69

function wide_class_arm(value: AnyWide) -> int {
    match (value) {
        Wide00 => 0,
        Wide01 => 100,
        Wide02 => 200,
        Wide03 => 300,
        Wide04 => 400,
        Wide05 => 500,
        Wide06 => 600,
        Wide07 => 700,
        Wide08 => 800,
        Wide09 => 900,
        Wide10 => 1000,
        Wide11 => 1100,
        Wide12 => 1200,
        Wide13 => 1300,
        Wide14 => 1400,
        Wide15 => 1500,
        Wide16 => 1600,
        Wide17 => 1700,
        Wide18 => 1800,
        Wide19 => 1900,
        Wide20 => 2000,
        Wide21 => 2100,
        Wide22 => 2200,
        Wide23 => 2300,
        Wide24 => 2400,
        Wide25 => 2500,
        Wide26 => 2600,
        Wide27 => 2700,
        Wide28 => 2800,
        Wide29 => 2900,
        Wide30 => 3000,
        Wide31 => 3100,
        Wide32 => 3200,
        Wide33 => 3300,
        Wide34 => 3400,
        Wide35 => 3500,
        Wide36 => 3600,
        Wide37 => 3700,
        Wide38 => 3800,
        Wide39 => 3900,
        Wide40 => 4000,
        Wide41 => 4100,
        Wide42 => 4200,
        Wide43 => 4300,
        Wide44 => 4400,
        Wide45 => 4500,
        Wide46 => 4600,
        Wide47 => 4700,
        Wide48 => 4800,
        Wide49 => 4900,
        Wide50 => 5000,
        Wide51 => 5100,
        Wide52 => 5200,
        Wide53 => 5300,
        Wide54 => 5400,
        Wide55 => 5500,
        Wide56 => 5600,
        Wide57 => 5700,
        Wide58 => 5800,
        Wide59 => 5900,
        Wide60 => 6000,
        Wide61 => 6100,
        Wide62 => 6200,
        Wide63 => 6300,
        Wide64 => 6400,
        Wide65 => 6500,
        Wide66 => 6600,
        Wide67 => 6700,
        Wide68 => 6800,
        Wide69 => 6900,
    }
}

test "match_seventy_class_arms_each_select_their_own" {
    assert.equal(wide_class_arm(Wide00 { weight: 0 }), 0)
    assert.equal(wide_class_arm(Wide01 { weight: 1 }), 100)
    assert.equal(wide_class_arm(Wide02 { weight: 2 }), 200)
    assert.equal(wide_class_arm(Wide03 { weight: 3 }), 300)
    assert.equal(wide_class_arm(Wide04 { weight: 4 }), 400)
    assert.equal(wide_class_arm(Wide05 { weight: 5 }), 500)
    assert.equal(wide_class_arm(Wide06 { weight: 6 }), 600)
    assert.equal(wide_class_arm(Wide07 { weight: 0 }), 700)
    assert.equal(wide_class_arm(Wide08 { weight: 1 }), 800)
    assert.equal(wide_class_arm(Wide09 { weight: 2 }), 900)
    assert.equal(wide_class_arm(Wide10 { weight: 3 }), 1000)
    assert.equal(wide_class_arm(Wide11 { weight: 4 }), 1100)
    assert.equal(wide_class_arm(Wide12 { weight: 5 }), 1200)
    assert.equal(wide_class_arm(Wide13 { weight: 6 }), 1300)
    assert.equal(wide_class_arm(Wide14 { weight: 0 }), 1400)
    assert.equal(wide_class_arm(Wide15 { weight: 1 }), 1500)
    assert.equal(wide_class_arm(Wide16 { weight: 2 }), 1600)
    assert.equal(wide_class_arm(Wide17 { weight: 3 }), 1700)
    assert.equal(wide_class_arm(Wide18 { weight: 4 }), 1800)
    assert.equal(wide_class_arm(Wide19 { weight: 5 }), 1900)
    assert.equal(wide_class_arm(Wide20 { weight: 6 }), 2000)
    assert.equal(wide_class_arm(Wide21 { weight: 0 }), 2100)
    assert.equal(wide_class_arm(Wide22 { weight: 1 }), 2200)
    assert.equal(wide_class_arm(Wide23 { weight: 2 }), 2300)
    assert.equal(wide_class_arm(Wide24 { weight: 3 }), 2400)
    assert.equal(wide_class_arm(Wide25 { weight: 4 }), 2500)
    assert.equal(wide_class_arm(Wide26 { weight: 5 }), 2600)
    assert.equal(wide_class_arm(Wide27 { weight: 6 }), 2700)
    assert.equal(wide_class_arm(Wide28 { weight: 0 }), 2800)
    assert.equal(wide_class_arm(Wide29 { weight: 1 }), 2900)
    assert.equal(wide_class_arm(Wide30 { weight: 2 }), 3000)
    assert.equal(wide_class_arm(Wide31 { weight: 3 }), 3100)
    assert.equal(wide_class_arm(Wide32 { weight: 4 }), 3200)
    assert.equal(wide_class_arm(Wide33 { weight: 5 }), 3300)
    assert.equal(wide_class_arm(Wide34 { weight: 6 }), 3400)
    assert.equal(wide_class_arm(Wide35 { weight: 0 }), 3500)
    assert.equal(wide_class_arm(Wide36 { weight: 1 }), 3600)
    assert.equal(wide_class_arm(Wide37 { weight: 2 }), 3700)
    assert.equal(wide_class_arm(Wide38 { weight: 3 }), 3800)
    assert.equal(wide_class_arm(Wide39 { weight: 4 }), 3900)
    assert.equal(wide_class_arm(Wide40 { weight: 5 }), 4000)
    assert.equal(wide_class_arm(Wide41 { weight: 6 }), 4100)
    assert.equal(wide_class_arm(Wide42 { weight: 0 }), 4200)
    assert.equal(wide_class_arm(Wide43 { weight: 1 }), 4300)
    assert.equal(wide_class_arm(Wide44 { weight: 2 }), 4400)
    assert.equal(wide_class_arm(Wide45 { weight: 3 }), 4500)
    assert.equal(wide_class_arm(Wide46 { weight: 4 }), 4600)
    assert.equal(wide_class_arm(Wide47 { weight: 5 }), 4700)
    assert.equal(wide_class_arm(Wide48 { weight: 6 }), 4800)
    assert.equal(wide_class_arm(Wide49 { weight: 0 }), 4900)
    assert.equal(wide_class_arm(Wide50 { weight: 1 }), 5000)
    assert.equal(wide_class_arm(Wide51 { weight: 2 }), 5100)
    assert.equal(wide_class_arm(Wide52 { weight: 3 }), 5200)
    assert.equal(wide_class_arm(Wide53 { weight: 4 }), 5300)
    assert.equal(wide_class_arm(Wide54 { weight: 5 }), 5400)
    assert.equal(wide_class_arm(Wide55 { weight: 6 }), 5500)
    assert.equal(wide_class_arm(Wide56 { weight: 0 }), 5600)
    assert.equal(wide_class_arm(Wide57 { weight: 1 }), 5700)
    assert.equal(wide_class_arm(Wide58 { weight: 2 }), 5800)
    assert.equal(wide_class_arm(Wide59 { weight: 3 }), 5900)
    assert.equal(wide_class_arm(Wide60 { weight: 4 }), 6000)
    assert.equal(wide_class_arm(Wide61 { weight: 5 }), 6100)
    assert.equal(wide_class_arm(Wide62 { weight: 6 }), 6200)
    assert.equal(wide_class_arm(Wide63 { weight: 0 }), 6300)
    assert.equal(wide_class_arm(Wide64 { weight: 1 }), 6400)
    assert.equal(wide_class_arm(Wide65 { weight: 2 }), 6500)
    assert.equal(wide_class_arm(Wide66 { weight: 3 }), 6600)
    assert.equal(wide_class_arm(Wide67 { weight: 4 }), 6700)
    assert.equal(wide_class_arm(Wide68 { weight: 5 }), 6800)
    assert.equal(wide_class_arm(Wide69 { weight: 6 }), 6900)
}

// The same arms with a wildcard: a value of a class outside the switch takes
// it.
class WideStranger { weight int }

function wide_class_arm_or_other(value: AnyWide | WideStranger) -> int {
    match (value) {
        Wide00 => 0,
        Wide01 => 100,
        Wide02 => 200,
        Wide03 => 300,
        Wide04 => 400,
        Wide05 => 500,
        Wide06 => 600,
        Wide07 => 700,
        Wide08 => 800,
        Wide09 => 900,
        Wide10 => 1000,
        Wide11 => 1100,
        Wide12 => 1200,
        Wide13 => 1300,
        Wide14 => 1400,
        Wide15 => 1500,
        Wide16 => 1600,
        Wide17 => 1700,
        Wide18 => 1800,
        Wide19 => 1900,
        Wide20 => 2000,
        Wide21 => 2100,
        Wide22 => 2200,
        Wide23 => 2300,
        Wide24 => 2400,
        Wide25 => 2500,
        Wide26 => 2600,
        Wide27 => 2700,
        Wide28 => 2800,
        Wide29 => 2900,
        Wide30 => 3000,
        Wide31 => 3100,
        Wide32 => 3200,
        Wide33 => 3300,
        Wide34 => 3400,
        Wide35 => 3500,
        Wide36 => 3600,
        Wide37 => 3700,
        Wide38 => 3800,
        Wide39 => 3900,
        Wide40 => 4000,
        Wide41 => 4100,
        Wide42 => 4200,
        Wide43 => 4300,
        Wide44 => 4400,
        Wide45 => 4500,
        Wide46 => 4600,
        Wide47 => 4700,
        Wide48 => 4800,
        Wide49 => 4900,
        Wide50 => 5000,
        Wide51 => 5100,
        Wide52 => 5200,
        Wide53 => 5300,
        Wide54 => 5400,
        Wide55 => 5500,
        Wide56 => 5600,
        Wide57 => 5700,
        Wide58 => 5800,
        Wide59 => 5900,
        Wide60 => 6000,
        Wide61 => 6100,
        Wide62 => 6200,
        Wide63 => 6300,
        Wide64 => 6400,
        Wide65 => 6500,
        Wide66 => 6600,
        Wide67 => 6700,
        Wide68 => 6800,
        Wide69 => 6900,
        _ => -1,
    }
}

test "match_seventy_class_arms_leave_a_stranger_to_the_wildcard" {
    assert.equal(wide_class_arm_or_other(WideStranger { weight: 3 }), -1)
    assert.equal(wide_class_arm_or_other(Wide00 { weight: 3 }), 0)
    assert.equal(wide_class_arm_or_other(Wide69 { weight: 3 }), 6900)
}

// ============================================================================
// Seventy sparse integer arms
// ============================================================================

function wide_int_arm(value: int) -> int {
    match (value) {
        5 => 0,
        42 => 1,
        153 => 2,
        338 => 3,
        597 => 4,
        930 => 5,
        1337 => 6,
        1818 => 7,
        2373 => 8,
        3002 => 9,
        3705 => 10,
        4482 => 11,
        5333 => 12,
        6258 => 13,
        7257 => 14,
        8330 => 15,
        9477 => 16,
        10698 => 17,
        11993 => 18,
        13362 => 19,
        14805 => 20,
        16322 => 21,
        17913 => 22,
        19578 => 23,
        21317 => 24,
        23130 => 25,
        25017 => 26,
        26978 => 27,
        29013 => 28,
        31122 => 29,
        33305 => 30,
        35562 => 31,
        37893 => 32,
        40298 => 33,
        42777 => 34,
        45330 => 35,
        47957 => 36,
        50658 => 37,
        53433 => 38,
        56282 => 39,
        59205 => 40,
        62202 => 41,
        65273 => 42,
        68418 => 43,
        71637 => 44,
        74930 => 45,
        78297 => 46,
        81738 => 47,
        85253 => 48,
        88842 => 49,
        92505 => 50,
        96242 => 51,
        100053 => 52,
        103938 => 53,
        107897 => 54,
        111930 => 55,
        116037 => 56,
        120218 => 57,
        124473 => 58,
        128802 => 59,
        133205 => 60,
        137682 => 61,
        142233 => 62,
        146858 => 63,
        151557 => 64,
        156330 => 65,
        161177 => 66,
        166098 => 67,
        171093 => 68,
        176162 => 69,
        _ => -1,
    }
}

test "match_seventy_sparse_int_arms_each_select_their_own" {
    assert.equal(wide_int_arm(5), 0)
    assert.equal(wide_int_arm(42), 1)
    assert.equal(wide_int_arm(153), 2)
    assert.equal(wide_int_arm(338), 3)
    assert.equal(wide_int_arm(597), 4)
    assert.equal(wide_int_arm(930), 5)
    assert.equal(wide_int_arm(1337), 6)
    assert.equal(wide_int_arm(1818), 7)
    assert.equal(wide_int_arm(2373), 8)
    assert.equal(wide_int_arm(3002), 9)
    assert.equal(wide_int_arm(3705), 10)
    assert.equal(wide_int_arm(4482), 11)
    assert.equal(wide_int_arm(5333), 12)
    assert.equal(wide_int_arm(6258), 13)
    assert.equal(wide_int_arm(7257), 14)
    assert.equal(wide_int_arm(8330), 15)
    assert.equal(wide_int_arm(9477), 16)
    assert.equal(wide_int_arm(10698), 17)
    assert.equal(wide_int_arm(11993), 18)
    assert.equal(wide_int_arm(13362), 19)
    assert.equal(wide_int_arm(14805), 20)
    assert.equal(wide_int_arm(16322), 21)
    assert.equal(wide_int_arm(17913), 22)
    assert.equal(wide_int_arm(19578), 23)
    assert.equal(wide_int_arm(21317), 24)
    assert.equal(wide_int_arm(23130), 25)
    assert.equal(wide_int_arm(25017), 26)
    assert.equal(wide_int_arm(26978), 27)
    assert.equal(wide_int_arm(29013), 28)
    assert.equal(wide_int_arm(31122), 29)
    assert.equal(wide_int_arm(33305), 30)
    assert.equal(wide_int_arm(35562), 31)
    assert.equal(wide_int_arm(37893), 32)
    assert.equal(wide_int_arm(40298), 33)
    assert.equal(wide_int_arm(42777), 34)
    assert.equal(wide_int_arm(45330), 35)
    assert.equal(wide_int_arm(47957), 36)
    assert.equal(wide_int_arm(50658), 37)
    assert.equal(wide_int_arm(53433), 38)
    assert.equal(wide_int_arm(56282), 39)
    assert.equal(wide_int_arm(59205), 40)
    assert.equal(wide_int_arm(62202), 41)
    assert.equal(wide_int_arm(65273), 42)
    assert.equal(wide_int_arm(68418), 43)
    assert.equal(wide_int_arm(71637), 44)
    assert.equal(wide_int_arm(74930), 45)
    assert.equal(wide_int_arm(78297), 46)
    assert.equal(wide_int_arm(81738), 47)
    assert.equal(wide_int_arm(85253), 48)
    assert.equal(wide_int_arm(88842), 49)
    assert.equal(wide_int_arm(92505), 50)
    assert.equal(wide_int_arm(96242), 51)
    assert.equal(wide_int_arm(100053), 52)
    assert.equal(wide_int_arm(103938), 53)
    assert.equal(wide_int_arm(107897), 54)
    assert.equal(wide_int_arm(111930), 55)
    assert.equal(wide_int_arm(116037), 56)
    assert.equal(wide_int_arm(120218), 57)
    assert.equal(wide_int_arm(124473), 58)
    assert.equal(wide_int_arm(128802), 59)
    assert.equal(wide_int_arm(133205), 60)
    assert.equal(wide_int_arm(137682), 61)
    assert.equal(wide_int_arm(142233), 62)
    assert.equal(wide_int_arm(146858), 63)
    assert.equal(wide_int_arm(151557), 64)
    assert.equal(wide_int_arm(156330), 65)
    assert.equal(wide_int_arm(161177), 66)
    assert.equal(wide_int_arm(166098), 67)
    assert.equal(wide_int_arm(171093), 68)
    assert.equal(wide_int_arm(176162), 69)
}

test "match_seventy_sparse_int_arms_leave_other_values_to_the_wildcard" {
    assert.equal(wide_int_arm(0), -1)
    assert.equal(wide_int_arm(4), -1)
    assert.equal(wide_int_arm(6), -1)
    assert.equal(wide_int_arm(43), -1)
    assert.equal(wide_int_arm(176163), -1)
    assert.equal(wide_int_arm(-5), -1)
}
