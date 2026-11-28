import normalizer.intrinsics.N;
class Base { int value; Base() { throw new IllegalStateException(); } }
class CanonicalFolds {
    static final CanonicalFolds INSTANCE = new CanonicalFolds();

    @N.EntryState Base seed = new Base();
    int fromState(int[] values) {
        int initial = seed.value;
        int result;
        /* @normalizer-ir result = sum from values as item yield item + initial */
        return result;
    }
}
