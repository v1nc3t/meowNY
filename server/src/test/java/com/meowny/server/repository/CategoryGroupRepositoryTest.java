package com.meowny.server.repository;

import com.meowny.server.entity.CategoryGroup;
import com.meowny.server.entity.User;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.jdbc.AutoConfigureTestDatabase;
import org.springframework.boot.test.autoconfigure.orm.jpa.DataJpaTest;
import org.springframework.boot.test.autoconfigure.orm.jpa.TestEntityManager;

import java.util.List;
import java.util.Optional;

import static org.assertj.core.api.Assertions.assertThat;

@DataJpaTest
@AutoConfigureTestDatabase(replace = AutoConfigureTestDatabase.Replace.NONE)
public class CategoryGroupRepositoryTest extends AbstractIntegrationTest {

    @Autowired
    private CategoryGroupRepository categoryGroupRepository;

    @Autowired
    private TestEntityManager entityManager;

    private User savedUser;

    @BeforeEach
    void setUp() {
        User user = new User();
        user.setFirstName("Jane");
        user.setLastName("Doe");
        user.setUsername("janedoe_groups");
        user.setEmail("jane.groups@example.com");
        user.setPassword("password123");

        savedUser = entityManager.persistAndFlush(user);
    }

    @AfterEach
    void tearDown() {
        categoryGroupRepository.deleteAllInBatch();
    }

    @Test
    @DisplayName("Should find all category groups belonging to a specific user")
    void shouldFindByUserId() {
        CategoryGroup group1 = createGroup("Essentials", savedUser);
        CategoryGroup group2 = createGroup("Lifestyle", savedUser);
        entityManager.persist(group1);
        entityManager.persist(group2);
        entityManager.flush();

        List<CategoryGroup> groups = categoryGroupRepository.findByUserId(savedUser.getId());

        assertThat(groups).hasSize(2);
        assertThat(groups)
                .extracting("name")
                .containsExactlyInAnyOrder("Essentials", "Lifestyle");
    }

    @Test
    @DisplayName("Should find a category group by name case-insensitively")
    void shouldFindByUserIdAndNameIgnoreCase() {
        CategoryGroup group = createGroup("Housing", savedUser);
        entityManager.persistAndFlush(group);

        Optional<CategoryGroup> foundLower = categoryGroupRepository.findByUserIdAndNameIgnoreCase(savedUser.getId(), "housing");
        Optional<CategoryGroup> foundUpper = categoryGroupRepository.findByUserIdAndNameIgnoreCase(savedUser.getId(), "HOUSING");

        assertThat(foundLower).isPresent();
        assertThat(foundUpper).isPresent();
        assertThat(foundLower.get().getName()).isEqualTo("Housing");
    }

    @Test
    @DisplayName("Should return empty optional when category group name does not match")
    void shouldReturnEmptyWhenNameNotFound() {
        Optional<CategoryGroup> found = categoryGroupRepository.findByUserIdAndNameIgnoreCase(savedUser.getId(), "Missing");

        assertThat(found).isEmpty();
    }

    private CategoryGroup createGroup(String name, User user) {
        CategoryGroup group = new CategoryGroup();
        group.setName(name);
        group.setUser(user);
        return group;
    }
}
