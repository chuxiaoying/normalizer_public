import normalizer.intrinsics.N;
class Base { int value; }
class CanonicalFolds {
    static final CanonicalFolds INSTANCE = new CanonicalFolds();

    @N.EntryState Base seed = new Base();
    int fromState(int[] values) {
        int before = seed.value;
        int result;
        /* @normalizer-ir
           result = reduce values from before as (acc, item) yield acc * 2 + item
        */
        return result;
    }
}
