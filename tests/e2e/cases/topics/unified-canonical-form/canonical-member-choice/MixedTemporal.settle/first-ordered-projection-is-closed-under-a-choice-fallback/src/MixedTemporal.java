class MixedTemporal {
    static int settle(int[] options) {
        Bill[] rows;
        /* @normalizer-ir rows = from bill as b yield { amount: b.amount } */
        int total;
        /* @normalizer-ir total = first from rows as r order by r.amount yield r.amount else 0 */
        int selected;
        /* @normalizer-ir selected = choose from options else total */
        return selected;
    }
}
class Bill { int amount; }
