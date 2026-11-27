class CanonicalPick {
    static int total(int[] values, int backup) {
        int[] local = new int[0]; int first;
        /* @normalizer-ir first = choose from local else backup */
        int second;
        /* @normalizer-ir second = choose from local else backup */
        return first + second;
    }
}
