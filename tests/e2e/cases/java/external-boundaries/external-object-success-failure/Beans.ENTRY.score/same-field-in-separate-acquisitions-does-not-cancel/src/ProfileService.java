import normalizer.intrinsics.N;

class Profile {
    int score;
}

class ProfileService {
    @N.External
    private final ProfileClient client;

    ProfileService(ProfileClient client) {
        this.client = client;
    }

    int score(int accountId) {
        Profile first = client.fetch(accountId); Profile second = client.fetch(accountId); return first.score - second.score;
    }
}

class ProfileClient {
    native Profile fetch(int accountId);
}

class ProfileUnavailable extends RuntimeException {}

class Beans {
    static final ProfileService ENTRY = new ProfileService(new ProfileClient());
}
