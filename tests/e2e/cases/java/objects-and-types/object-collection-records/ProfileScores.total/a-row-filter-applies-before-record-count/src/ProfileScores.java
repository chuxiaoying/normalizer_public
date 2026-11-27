class Profile {
    int score;
}

class ProfileScores {
    static int total(Profile[] profiles) {
        int total;
        /* @normalizer-ir
           total = count from profiles as profile where profile.score > 0
        */
        return total;
    }
}
