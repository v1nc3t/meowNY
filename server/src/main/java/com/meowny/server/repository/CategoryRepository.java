package com.meowny.server.repository;

import com.meowny.commons.entity.Category;
import com.meowny.commons.entity.TransactionType;
import org.springframework.data.jpa.repository.JpaRepository;

import java.util.List;
import java.util.Optional;

public interface CategoryRepository extends JpaRepository<Category, Long> {

    List<Category> findByUserId(Long userId);

    List<Category> findByUserIdAndType(Long userId, TransactionType type);

    Optional<Category> findByUserIdAndNameIgnoreCase(Long userId, String name);
}
