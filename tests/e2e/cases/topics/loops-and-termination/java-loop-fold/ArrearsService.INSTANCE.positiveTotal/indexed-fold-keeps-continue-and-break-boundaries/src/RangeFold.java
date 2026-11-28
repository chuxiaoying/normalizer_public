class RangeFold {
    static int total(int hi) {
        int total = 0;
        /* @normalizer-ir
           total = fold [0, hi) as i when i < 1 continue when i > 3 break yield i
        */
        return total;
    }
}
