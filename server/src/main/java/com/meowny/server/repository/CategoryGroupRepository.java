package com.meowny.server.repository;

import com.meowny.server.entity.CategoryGroup;
import org.springframework.data.jpa.repository.JpaRepository;

import java.util.List;
import java.util.Optional;

public interface CategoryGroupRepository extends JpaRepository<CategoryGroup, Long> {

    List<CategoryGroup> findByUserId(Long userId);

    Optional<CategoryGroup> findByUserIdAndNameIgnoreCase(Long userId, String name);
}
