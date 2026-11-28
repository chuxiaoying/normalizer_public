class MixedTemporal {
    static int settle(int[] options, int hi, int threshold) {
        int total;
        /* @normalizer-ir total = sum from [1, hi) as i where i > threshold yield i * i */
        int selected;
        /* @normalizer-ir selected = choose from options else total */
        return selected;
    }
}
