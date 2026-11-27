import normalizer.intrinsics.N;
class GuardedResult {
    static final GuardedResult INSTANCE = new GuardedResult();
    @N.EntryState int result;
    static int pure(int a, int b) { if (a != 1 || b != 2) return 9; return 7; }
    int state(int a, int b) { result = a == 1 && b == 2 ? 7 : 9; return result; }
}
