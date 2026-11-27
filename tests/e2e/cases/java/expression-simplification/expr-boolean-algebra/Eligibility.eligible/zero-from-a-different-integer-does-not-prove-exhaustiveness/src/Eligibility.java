class Eligibility {
    static boolean eligible(int score, int alternate) {
        boolean active = score >= 60;
        return score > 0 || score < 0 || alternate == 0;
    }
}
