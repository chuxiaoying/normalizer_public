class MixedTemporal {
    static int settle(int[] options, boolean enabled, int backup) {
        if (!enabled) return backup;
        Bill[] rows;
        /* @normalizer-ir rows = from bill as b yield { amount: b.amount } */
        int total;
        /* @normalizer-ir total = count from rows as r where r.amount > 0 */
        int selected;
        /* @normalizer-ir selected = choose from options else total */
        return selected;
    }
}
class Bill { int amount; }
