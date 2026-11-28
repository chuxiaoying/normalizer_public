class CanonicalFolds {
    static int difference(int[] left, int[] right, int initial) {
        int leftValue;
        /* @normalizer-ir leftValue = sum from left as digit yield digit + initial */
        int rightValue;
        /* @normalizer-ir rightValue = sum from right as rightItem yield rightItem + initial */
        return leftValue - rightValue;
    }
}
