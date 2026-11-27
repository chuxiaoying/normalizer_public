class CanonicalPick {
    static int total(int[] values, int backup) {
        int first;
        /* @normalizer-ir first = choose from values else backup */
        int second;
        /* @normalizer-ir second = choose from values else "not a number" */
        return first + second;
    }
}
