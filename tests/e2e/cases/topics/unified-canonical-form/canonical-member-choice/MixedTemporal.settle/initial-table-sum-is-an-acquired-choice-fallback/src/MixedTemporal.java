class MixedTemporal {
    static int settle(int[] options) {
        Bill[] rows;
        /* @normalizer-ir rows = from bill as b yield { amount: b.amount } */
        int total;
        /* @normalizer-ir total = sum from rows as r yield r.amount */
        int selected;
        /* @normalizer-ir selected = choose from options else total */
        return selected;
    }
}
class Bill { int amount; }
