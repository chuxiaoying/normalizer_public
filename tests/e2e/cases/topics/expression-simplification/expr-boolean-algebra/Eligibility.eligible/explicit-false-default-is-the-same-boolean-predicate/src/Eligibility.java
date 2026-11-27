class Eligibility {
    static boolean eligible(int score, boolean preferred) {
        boolean active = score >= 60;
        return active ? active : false;
    }
}
