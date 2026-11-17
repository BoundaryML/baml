package main

func f0(n int) int  { return f1(n + 1) }
func f1(n int) int  { return f2(n + 1) }
func f2(n int) int  { return f3(n + 1) }
func f3(n int) int  { return f4(n + 1) }
func f4(n int) int  { return f5(n + 1) }
func f5(n int) int  { return f6(n + 1) }
func f6(n int) int  { return f7(n + 1) }
func f7(n int) int  { return f8(n + 1) }
func f8(n int) int  { return f9(n + 1) }
func f9(n int) int  { return f10(n + 1) }
func f10(n int) int { return f11(n + 1) }
func f11(n int) int { return f12(n + 1) }
func f12(n int) int { return f13(n + 1) }
func f13(n int) int { return f14(n + 1) }
func f14(n int) int { return f15(n + 1) }
func f15(n int) int { return f16(n + 1) }
func f16(n int) int { return f17(n + 1) }
func f17(n int) int { return f18(n + 1) }
func f18(n int) int { return f19(n + 1) }
func f19(n int) int { return f20(n + 1) }
func f20(n int) int { return f21(n + 1) }
func f21(n int) int { return f22(n + 1) }
func f22(n int) int { return f23(n + 1) }
func f23(n int) int { return f24(n + 1) }
func f24(n int) int { return f25(n + 1) }
func f25(n int) int { return f26(n + 1) }
func f26(n int) int { return f27(n + 1) }
func f27(n int) int { return f28(n + 1) }
func f28(n int) int { return f29(n + 1) }
func f29(n int) int { return f30(n + 1) }
func f30(n int) int { return f31(n + 1) }
func f31(n int) int { return f32(n + 1) }
func f32(n int) int { return f33(n + 1) }
func f33(n int) int { return f34(n + 1) }
func f34(n int) int { return f35(n + 1) }
func f35(n int) int { return f36(n + 1) }
func f36(n int) int { return f37(n + 1) }
func f37(n int) int { return f38(n + 1) }
func f38(n int) int { return f39(n + 1) }
func f39(n int) int { return f40(n + 1) }
func f40(n int) int { return f41(n + 1) }
func f41(n int) int { return f42(n + 1) }
func f42(n int) int { return f43(n + 1) }
func f43(n int) int { return f44(n + 1) }
func f44(n int) int { return f45(n + 1) }
func f45(n int) int { return f46(n + 1) }
func f46(n int) int { return f47(n + 1) }
func f47(n int) int { return f48(n + 1) }
func f48(n int) int { return f49(n + 1) }
func f49(n int) int { return f50(n + 1) }
func f50(n int) int { return f51(n + 1) }
func f51(n int) int { return f52(n + 1) }
func f52(n int) int { return f53(n + 1) }
func f53(n int) int { return f54(n + 1) }
func f54(n int) int { return f55(n + 1) }
func f55(n int) int { return f56(n + 1) }
func f56(n int) int { return f57(n + 1) }
func f57(n int) int { return f58(n + 1) }
func f58(n int) int { return f59(n + 1) }
func f59(n int) int { return f60(n + 1) }
func f60(n int) int { return f61(n + 1) }
func f61(n int) int { return f62(n + 1) }
func f62(n int) int { return f63(n + 1) }
func f63(n int) int { return f64(n + 1) }
func f64(n int) int { return f65(n + 1) }
func f65(n int) int { return f66(n + 1) }
func f66(n int) int { return f67(n + 1) }
func f67(n int) int { return f68(n + 1) }
func f68(n int) int { return f69(n + 1) }
func f69(n int) int { return f70(n + 1) }
func f70(n int) int { return f71(n + 1) }
func f71(n int) int { return f72(n + 1) }
func f72(n int) int { return f73(n + 1) }
func f73(n int) int { return f74(n + 1) }
func f74(n int) int { return f75(n + 1) }
func f75(n int) int { return f76(n + 1) }
func f76(n int) int { return f77(n + 1) }
func f77(n int) int { return f78(n + 1) }
func f78(n int) int { return f79(n + 1) }
func f79(n int) int { return f80(n + 1) }
func f80(n int) int { return f81(n + 1) }
func f81(n int) int { return f82(n + 1) }
func f82(n int) int { return f83(n + 1) }
func f83(n int) int { return f84(n + 1) }
func f84(n int) int { return f85(n + 1) }
func f85(n int) int { return f86(n + 1) }
func f86(n int) int { return f87(n + 1) }
func f87(n int) int { return f88(n + 1) }
func f88(n int) int { return f89(n + 1) }
func f89(n int) int { return f90(n + 1) }
func f90(n int) int { return f91(n + 1) }
func f91(n int) int { return f92(n + 1) }
func f92(n int) int { return f93(n + 1) }
func f93(n int) int { return f94(n + 1) }
func f94(n int) int { return f95(n + 1) }
func f95(n int) int { return f96(n + 1) }
func f96(n int) int { return f97(n + 1) }
func f97(n int) int { return f98(n + 1) }
func f98(n int) int { return f99(n + 1) }
func f99(n int) int { return n }

func Main() int {
	s := 0
	for i := 0; i < 10000; i++ {
		s += f0(0)
	}
	return s
}
