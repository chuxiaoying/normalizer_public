class CanonicalFolds {
    static int difference(int[] left, int[] right, int initial) {
        int[] same = left; int leftValue;
        /* @normalizer-ir
           leftValue = reduce same from initial as (leftAcc, leftItem) yield leftAcc * 2 + leftItem
        */
        int rightValue;
        /* @normalizer-ir
           rightValue = reduce right from initial as (rightAcc, rightItem) yield rightAcc * 2 + rightItem
        */
        return leftValue - rightValue;
    }
}
