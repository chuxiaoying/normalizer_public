class CanonicalFolds {
    static int difference(int[] left, int[] right, int initial) {
        int[] same = left; int leftValue;
        /* @normalizer-ir leftValue = sum from same as leftItem yield leftItem + initial */
        int rightValue;
        /* @normalizer-ir rightValue = sum from right as rightItem yield rightItem + initial */
        return leftValue - rightValue;
    }
}
