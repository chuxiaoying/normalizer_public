import normalizer.intrinsics.N;
class Base { int value; int offset; }
class CanonicalFolds {
    static final CanonicalFolds INSTANCE = new CanonicalFolds();

    @N.EntryState Base seed = new Base();
    int fromState(int[] values) {
        int initial = seed.offset;
        int result;
        /* @normalizer-ir
           result = reduce values from initial as (acc, item) yield acc * 2 + item
        */
        return result;
    }
}
