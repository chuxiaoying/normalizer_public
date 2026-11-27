class Profile {
    int score;
}

class ProfileScores {
    static int total(Profile[] profiles) {
        int total;
        /* @normalizer-ir
           total = sum from profiles as profile yield profile.score + 1
        */
        return total;
    }
}
