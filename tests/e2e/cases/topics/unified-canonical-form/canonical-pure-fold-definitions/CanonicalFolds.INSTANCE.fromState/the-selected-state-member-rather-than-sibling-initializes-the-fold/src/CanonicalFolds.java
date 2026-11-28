import normalizer.intrinsics.N;
class Base { int value; int offset; }
class CanonicalFolds {
    static final CanonicalFolds INSTANCE = new CanonicalFolds();

    @N.EntryState Base seed = new Base();
    int fromState(int[] values) {
        int initial = seed.offset;
        int result;
        /* @normalizer-ir result = sum from values as item yield item + initial */
        return result;
    }
}
