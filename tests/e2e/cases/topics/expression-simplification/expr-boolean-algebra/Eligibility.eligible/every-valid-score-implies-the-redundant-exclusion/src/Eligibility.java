class Eligibility {
    static boolean eligible(int score, boolean preferred) {
        boolean active = score >= 60;
        return (score == 1 || score == 2) && preferred && score != 3;
    }
}
