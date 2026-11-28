class CanonicalFolds {
    static int difference(int[] left, int[] right, int initial) {
        if (initial > 0) return foldInside(left, right, initial);
        return initial;
    }
    static int foldInside(int[] left, int[] right, int initial) {
        int leftValue;
        /* @normalizer-ir leftValue = sum from left as leftItem yield leftItem + initial */
        int rightValue;
        /* @normalizer-ir rightValue = sum from right as rightItem yield rightItem + initial */
        return leftValue - rightValue;
    }
}
