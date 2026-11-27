import normalizer.intrinsics.N;
class GuardedResult {
    static final GuardedResult INSTANCE = new GuardedResult();
    @N.EntryState int result;
    static int pure(int a, int b) { return a == 1 && b == 2 ? 7 : 9; }
    int state(int a, int b) { result = a != 1 || b != 2 ? 9 : 7; return result; }
}
