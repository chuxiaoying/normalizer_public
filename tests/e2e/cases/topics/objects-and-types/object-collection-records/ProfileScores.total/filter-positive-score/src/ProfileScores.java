class Profile {
    int score;
}

class ProfileScores {
    static int total(Profile[] profiles) {
        int total;
        /* @normalizer-ir
           total = sum from profiles as profile where profile.score > 0 yield profile.score
        */
        return total;
    }
}
