package com.meowny.server.service;

import com.meowny.server.dto.categorygroup.CategoryGroupResponse;
import com.meowny.server.dto.categorygroup.CreateCategoryGroupRequest;
import com.meowny.server.dto.categorygroup.UpdateCategoryGroupRequest;
import com.meowny.server.entity.CategoryGroup;
import com.meowny.server.entity.User;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.repository.CategoryGroupRepository;
import com.meowny.server.repository.UserRepository;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

import java.util.List;
import java.util.stream.Collectors;

@Service
public class CategoryGroupService {

    private final CategoryGroupRepository categoryGroupRepository;
    private final UserRepository userRepository;

    public CategoryGroupService(CategoryGroupRepository categoryGroupRepository,
                                UserRepository userRepository) {
        this.categoryGroupRepository = categoryGroupRepository;
        this.userRepository = userRepository;
    }

    @Transactional(readOnly = true)
    public List<CategoryGroupResponse> getCategoryGroupsByUserId(Long userId) {
        return categoryGroupRepository.findByUserId(userId)
                .stream()
                .map(this::mapToResponse)
                .collect(Collectors.toList());
    }

    @Transactional
    public CategoryGroupResponse createCategoryGroup(CreateCategoryGroupRequest request) {
        User user = userRepository.findById(request.userId())
                .orElseThrow(() -> new IllegalArgumentException("User not found with ID: " + request.userId()));

        categoryGroupRepository.findByUserIdAndNameIgnoreCase(request.userId(), request.name())
                .ifPresent(existing -> {
                    throw new ResourceConflictException("A category group with the name '" + request.name() + "' already exists.");
                });

        CategoryGroup group = new CategoryGroup();
        group.setUser(user);
        group.setName(request.name());

        CategoryGroup savedGroup = categoryGroupRepository.save(group);
        return mapToResponse(savedGroup);
    }

    @Transactional
    public CategoryGroupResponse updateCategoryGroup(Long id, UpdateCategoryGroupRequest request) {
        CategoryGroup group = categoryGroupRepository.findById(id)
                .orElseThrow(() -> new IllegalArgumentException("Category group not found with ID: " + id));

        if (!group.getName().equalsIgnoreCase(request.name())) {
            categoryGroupRepository.findByUserIdAndNameIgnoreCase(group.getUser().getId(), request.name())
                    .ifPresent(existing -> {
                        throw new ResourceConflictException("Another category group with the name '" + request.name() + "' already exists.");
                    });
            group.setName(request.name());
        }

        CategoryGroup updatedGroup = categoryGroupRepository.save(group);
        return mapToResponse(updatedGroup);
    }

    @Transactional
    public void deleteCategoryGroup(Long id) {
        if (!categoryGroupRepository.existsById(id)) {
            throw new IllegalArgumentException("Category group not found with ID: " + id);
        }
        categoryGroupRepository.deleteById(id);
    }

    private CategoryGroupResponse mapToResponse(CategoryGroup group) {
        return new CategoryGroupResponse(
                group.getId(),
                group.getUser().getId(),
                group.getName(),
                group.getCreatedAt()
        );
    }
}
