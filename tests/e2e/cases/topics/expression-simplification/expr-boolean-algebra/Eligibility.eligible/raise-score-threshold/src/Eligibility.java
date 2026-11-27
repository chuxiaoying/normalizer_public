class Eligibility {
    static boolean eligible(int score, boolean preferred) {
        boolean active = score >= 70;
        return (active && preferred) || (active && !preferred);
    }
}
