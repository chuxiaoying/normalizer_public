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
        Profile reply0 = client.fetch(accountId);
Profile reply1 = client.fetch(accountId + 1);
Profile reply2 = client.fetch(accountId + 2);
Profile reply3 = client.fetch(accountId + 3);
Profile reply4 = client.fetch(accountId + 4);
Profile reply5 = client.fetch(accountId + 5);
Profile reply6 = client.fetch(accountId + 6);
Profile reply7 = client.fetch(accountId + 7);
Profile reply8 = client.fetch(accountId + 8);
Profile reply9 = client.fetch(accountId + 9);
Profile reply10 = client.fetch(accountId + 10);
return reply10.score + reply2.score;
    }
}

class ProfileClient {
    native Profile fetch(int accountId);
}

class ProfileUnavailable extends RuntimeException {}

class Beans {
    static final ProfileService ENTRY = new ProfileService(new ProfileClient());
}
