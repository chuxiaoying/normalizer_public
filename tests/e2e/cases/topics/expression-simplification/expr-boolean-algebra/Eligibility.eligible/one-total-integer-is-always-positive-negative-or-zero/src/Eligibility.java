class Eligibility {
    static boolean eligible(int score, boolean preferred) {
        boolean active = score >= 60;
        return score > 0 || score < 0 || score == 0;
    }
}
