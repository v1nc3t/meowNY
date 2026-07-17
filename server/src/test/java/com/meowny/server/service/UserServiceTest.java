package com.meowny.server.service;

import com.meowny.commons.entity.User;
import com.meowny.server.dto.user.CreateUserRequest;
import com.meowny.server.dto.user.UpdateUserRequest;
import com.meowny.server.dto.user.UserResponse;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.repository.UserRepository;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.InjectMocks;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;
import org.springframework.security.crypto.bcrypt.BCryptPasswordEncoder;

import java.util.Optional;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class UserServiceTest {

    @Mock
    private UserRepository userRepository;

    @Mock
    private BCryptPasswordEncoder passwordEncoder;

    @InjectMocks
    private UserService userService;

    // ==========================================
    // getUserById BRANCHES
    // ==========================================

    @Test
    @DisplayName("getUserById: Should return UserResponse when user exists")
    void getUserById_UserExists_ReturnsResponse() {
        Long userId = 1L;
        User user = new User();
        user.setId(userId);
        user.setEmail("test@example.com");

        when(userRepository.findById(userId)).thenReturn(Optional.of(user));

        UserResponse response = userService.getUserById(userId);

        assertThat(response).isNotNull();
        assertThat(response.id()).isEqualTo(userId);
        verify(userRepository).findById(userId);
    }

    @Test
    @DisplayName("getUserById: Should throw IllegalArgumentException when user does not exist")
    void getUserById_UserDoesNotExist_ThrowsException() {
        Long userId = 1L;
        when(userRepository.findById(userId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> userService.getUserById(userId))
                .isInstanceOf(IllegalArgumentException.class)
                assertThatThrownBy(() -> userService.getUserById(userId))
                        .isInstanceOf(IllegalArgumentException.class)
                        .hasMessageContaining("User not found with ID: " + userId);
    // ==========================================
    // createUser BRANCHES
    // ==========================================

    @Test
    @DisplayName("createUser: Should create user successfully when email and username are unique")
    void createUser_ValidRequest_CreatesUser() {
        CreateUserRequest request = new CreateUserRequest("John", "Doe", "john@example.com", "johndoe", "pass");
        User savedUser = new User();
        savedUser.setId(1L);
        savedUser.setEmail(request.email());

        when(userRepository.findUserByEmail(request.email())).thenReturn(Optional.empty());
        when(userRepository.findUserByUsername(request.username())).thenReturn(Optional.empty());
        when(passwordEncoder.encode(request.password())).thenReturn("hashed_pass");
        when(userRepository.save(any(User.class))).thenReturn(savedUser);

        UserResponse response = userService.createUser(request);

        assertThat(response).isNotNull();
        verify(userRepository).save(any(User.class));
    }

    @Test
    @DisplayName("createUser: Should throw ResourceConflictException when email is taken")
    void createUser_EmailExists_ThrowsResourceConflictException() {
        CreateUserRequest request = new CreateUserRequest("John", "Doe", "taken@example.com", "johndoe", "pass");
        when(userRepository.findUserByEmail(request.email())).thenReturn(Optional.of(new User()));

        assertThatThrownBy(() -> userService.createUser(request))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("An account with this email address already exists.");

        verify(userRepository, never()).findUserByUsername(any());
        verifyNoInteractions(passwordEncoder);
    }

    @Test
    @DisplayName("createUser: Should throw ResourceConflictException when username is taken")
    void createUser_UsernameExists_ThrowsResourceConflictException() {
        CreateUserRequest request = new CreateUserRequest("John", "Doe", "john@example.com", "taken_user", "pass");
        when(userRepository.findUserByEmail(request.email())).thenReturn(Optional.empty());
        when(userRepository.findUserByUsername(request.username())).thenReturn(Optional.of(new User()));

        assertThatThrownBy(() -> userService.createUser(request))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("This username is already taken.");

        verify(userRepository, never()).save(any());
    }

    // ==========================================
    // updateUser BRANCHES
    // ==========================================

    @Test
    @DisplayName("updateUser: Should throw IllegalArgumentException when target user to update is not found")
    void updateUser_UserNotFound_ThrowsException() {
        Long userId = 1L;
        UpdateUserRequest request = new UpdateUserRequest("John", "Doe", "john@example.com");
        when(userRepository.findById(userId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> userService.updateUser(userId, request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("User not found with ID: " + userId);
    }

    @Test
    @DisplayName("updateUser: Should update names without checking uniqueness if email is unchanged")
    void updateUser_EmailUnchanged_UpdatesSuccessfully() {
        Long userId = 1L;
        UpdateUserRequest request = new UpdateUserRequest("UpdatedFirst", "UpdatedLast", "same@example.com");

        User existingUser = new User();
        existingUser.setId(userId);
        existingUser.setEmail("same@example.com");

        when(userRepository.findById(userId)).thenReturn(Optional.of(existingUser));
        when(userRepository.save(any(User.class))).thenReturn(existingUser);

        UserResponse response = userService.updateUser(userId, request);

        // Then
        assertThat(response).isNotNull();
        verify(userRepository, never()).findUserByEmail(any());
        verify(userRepository).save(existingUser);
    }

    @Test
    @DisplayName("updateUser: Should check uniqueness and update email when email changes and is free")
    void updateUser_EmailChangedAndFree_UpdatesSuccessfully() {
        Long userId = 1L;
        UpdateUserRequest request = new UpdateUserRequest("John", "Doe", "new@example.com");

        User existingUser = new User();
        existingUser.setId(userId);
        existingUser.setEmail("old@example.com");

        when(userRepository.findById(userId)).thenReturn(Optional.of(existingUser));
        when(userRepository.findUserByEmail(request.email())).thenReturn(Optional.empty());
        when(userRepository.save(any(User.class))).thenReturn(existingUser);

        UserResponse response = userService.updateUser(userId, request);

        assertThat(response).isNotNull();
        verify(userRepository).findUserByEmail(request.email());
        verify(userRepository).save(existingUser);
    }

    @Test
    @DisplayName("updateUser: Should throw ResourceConflictException when changing to an already registered email")
    void updateUser_EmailChangedAndTaken_ThrowsResourceConflictException() {
        Long userId = 1L;
        UpdateUserRequest request = new UpdateUserRequest("John", "Doe", "taken@example.com");

        User existingUser = new User();
        existingUser.setId(userId);
        existingUser.setEmail("old@example.com");

        when(userRepository.findById(userId)).thenReturn(Optional.of(existingUser));
        when(userRepository.findUserByEmail(request.email())).thenReturn(Optional.of(new User()));

        // When & Then
        assertThatThrownBy(() -> userService.updateUser(userId, request))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("This email address is already registered to another user.");

        verify(userRepository, never()).save(any());
    }

    // ==========================================
    // deleteUser BRANCHES
    // ==========================================

    @Test
    @DisplayName("deleteUser: Should delete user successfully when user exists")
    void deleteUser_UserExists_DeletesSuccessfully() {
        Long userId = 1L;
        when(userRepository.existsById(userId)).thenReturn(true);

        userService.deleteUser(userId);

        verify(userRepository).deleteById(userId);
    }

    @Test
    @DisplayName("deleteUser: Should throw IllegalArgumentException when user does not exist")
    void deleteUser_UserDoesNotExist_ThrowsException() {
        Long userId = 1L;
        when(userRepository.existsById(userId)).thenReturn(false);

        assertThatThrownBy(() -> userService.deleteUser(userId))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("User not found with ID: " + userId);

        verify(userRepository, never()).deleteById(any());
    }
}