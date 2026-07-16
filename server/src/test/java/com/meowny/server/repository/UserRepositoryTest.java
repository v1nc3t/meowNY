package com.meowny.server.repository;

import com.meowny.commons.entity.User;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.jdbc.AutoConfigureTestDatabase;
import org.springframework.boot.test.autoconfigure.orm.jpa.DataJpaTest;
import org.springframework.boot.test.autoconfigure.orm.jpa.TestEntityManager;

import java.util.Optional;

import static org.assertj.core.api.Assertions.assertThat;

@DataJpaTest
@AutoConfigureTestDatabase(replace = AutoConfigureTestDatabase.Replace.NONE)
public class UserRepositoryTest extends AbstractIntegrationTest {

    @Autowired
    private UserRepository userRepository;

    @Autowired
    private TestEntityManager entityManager;

    private User defaultUser;

    @BeforeEach
    void setUp() {
        userRepository.deleteAll();

        defaultUser = createUser("Vincent", "Meowny", "vincent_m", "vincent@meowny.com", "securePass123");
        entityManager.persistAndFlush(defaultUser);
    }

    @AfterEach
    void tearDown() {
        userRepository.deleteAllInBatch();
    }

    @Test
    @DisplayName("Should find user by their unique username")
    void shouldFindUserByUsername() {
        Optional<User> foundUser = userRepository.findUserByUsername("vincent_m");

        assertThat(foundUser).isPresent();
        assertThat(foundUser.get().getEmail()).isEqualTo("vincent@meowny.com");
    }

    @Test
    @DisplayName("Should return empty when username does not exist")
    void shouldReturnEmptyWhenUsernameNotFound() {
        Optional<User> foundUser = userRepository.findUserByUsername("non_existent_user");

        assertThat(foundUser).isEmpty();
    }

    @Test
    @DisplayName("Should find user by their unique email address")
    void shouldFindUserByEmail() {
        Optional<User> foundUser = userRepository.findUserByEmail("vincent@meowny.com");

        assertThat(foundUser).isPresent();
        assertThat(foundUser.get().getUsername()).isEqualTo("vincent_m");
    }

    @Test
    @DisplayName("Should return empty when email does not exist")
    void shouldReturnEmptyWhenEmailNotFound() {
        Optional<User> foundUser = userRepository.findUserByEmail("unknown@example.com");

        assertThat(foundUser).isEmpty();
    }

    @Test
    @DisplayName("Should return true when verifying existence of a registered username")
    void shouldReturnTrueWhenUsernameExists() {
        boolean exists = userRepository.existsByUsername("vincent_m");

        assertThat(exists).isTrue();
    }

    @Test
    @DisplayName("Should return false when verifying existence of an unregistered username")
    void shouldReturnFalseWhenUsernameDoesNotExist() {
        boolean exists = userRepository.existsByUsername("random_stranger");

        assertThat(exists).isFalse();
    }

    @Test
    @DisplayName("Should return true when verifying existence of an already registered email")
    void shouldReturnTrueWhenEmailExists() {
        boolean exists = userRepository.existsByEmail("vincent@meowny.com");

        assertThat(exists).isTrue();
    }

    @Test
    @DisplayName("Should return false when verifying existence of an unregistered email")
    void shouldReturnFalseWhenEmailDoesNotExist() {
        boolean exists = userRepository.existsByEmail("nobody@nowhere.com");

        assertThat(exists).isFalse();
    }

    private User createUser(
            String firstName,
            String lastName,
            String username,
            String email,
            String password
    ) {
        User user = new User();
        user.setFirstName(firstName);
        user.setLastName(lastName);
        user.setUsername(username);
        user.setEmail(email);
        user.setPassword(password);
        return user;
    }
}
