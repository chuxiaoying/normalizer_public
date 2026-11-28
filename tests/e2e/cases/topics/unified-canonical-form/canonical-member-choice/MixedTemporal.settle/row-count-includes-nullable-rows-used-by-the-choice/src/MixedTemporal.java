import normalizer.intrinsics.N;
class MixedTemporal {
    static int settle(int[] options) {
        Bill[] rows;
        /* @normalizer-ir rows = from bill as b yield { label: b.label } */
        int total;
        /* @normalizer-ir total = count from rows as r */
        int selected;
        /* @normalizer-ir selected = choose from options else total */
        return selected;
    }
}
class Bill { @N.Nullable String label; }
