class MixedTemporal {
    static int settle(int[] options, int[][] groups, int threshold) {
        int acquired;
        /* @normalizer-ir acquired = sum from groups as group
               yield sum from group as value where value > threshold yield value
        */
        int selected;
        /* @normalizer-ir selected = choose from options else acquired */
        return selected;
    }
}
