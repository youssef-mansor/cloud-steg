// ============== State ==============
let currentUser = null;
let currentTab = 'myImages';

// ============== DOM Elements ==============
const loginScreen = document.getElementById('loginScreen');
const dashboardScreen = document.getElementById('dashboardScreen');
const usernameInput = document.getElementById('usernameInput');
const loginBtn = document.getElementById('loginBtn');
const logoutBtn = document.getElementById('logoutBtn');
const usernameDisplay = document.getElementById('usernameDisplay');
const heartbeatStatus = document.getElementById('heartbeatStatus');

// Tabs
const tabBtns = document.querySelectorAll('.tab-btn');
const tabPanes = document.querySelectorAll('.tab-pane');

// My Images Tab
const imageUpload = document.getElementById('imageUpload');
const myImagesGrid = document.getElementById('myImagesGrid');

// Browse Tab
const refreshUsersBtn = document.getElementById('refreshUsersBtn');
const usersList = document.getElementById('usersList');

// Requests Tab
const refreshRequestsBtn = document.getElementById('refreshRequestsBtn');
const requestsList = document.getElementById('requestsList');

// Viewable Tab
const refreshViewableBtn = document.getElementById('refreshViewableBtn');
const viewableList = document.getElementById('viewableList');

// Modals
const imageModal = document.getElementById('imageModal');
const modalImage = document.getElementById('modalImage');
const modalTitle = document.getElementById('modalTitle');
const modalDetails = document.getElementById('modalDetails');

const requestModal = document.getElementById('requestModal');
const requestImageName = document.getElementById('requestImageName');
const requestUsername = document.getElementById('requestUsername');
const sendRequestBtn = document.getElementById('sendRequestBtn');

const approveModal = document.getElementById('approveModal');
const approveImageName = document.getElementById('approveImageName');
const approveUsername = document.getElementById('approveUsername');
const viewCountInput = document.getElementById('viewCountInput');
const coverImageInput = document.getElementById('coverImageInput');
const approveBtn = document.getElementById('approveBtn');

// ============== API Helpers ==============

async function apiCall(endpoint, options = {}) {
    try {
        const response = await fetch(endpoint, {
            ...options,
            headers: {
                'Content-Type': 'application/json',
                ...options.headers
            }
        });

        if (!response.ok) {
            const error = await response.json().catch(() => ({ error: 'Request failed' }));
            throw new Error(error.error || 'Request failed');
        }

        return await response.json();
    } catch (e) {
        console.error('API Error:', e);
        alert(e.message);
        throw e;
    }
}

// ============== Authentication ==============

async function register() {
    const username = usernameInput.value.trim();

    if (!username) {
        alert('Please enter a username');
        return;
    }

    try {
        const data = await apiCall('/api/register', {
            method: 'POST',
            body: JSON.stringify({ username })
        });

        alert(`✅ User '${data.username || username}' registered successfully! You can now login.`);

    } catch (e) {
        console.error('Registration failed:', e);
    }
}

async function login() {
    const username = usernameInput.value.trim();

    if (!username) {
        alert('Please enter a username');
        return;
    }

    try {
        const data = await apiCall('/api/login', {
            method: 'POST',
            body: JSON.stringify({ username })
        });

        currentUser = data.username;
        usernameDisplay.textContent = `@${currentUser}`;

        loginScreen.classList.add('hidden');
        dashboardScreen.classList.remove('hidden');

        // Load initial data
        loadMyImages();
        loadOnlineUsers();

    } catch (e) {
        console.error('Login failed:', e);
    }
}

async function logout() {
    try {
        await apiCall('/api/logout', { method: 'POST' });
        currentUser = null;
        loginScreen.classList.remove('hidden');
        dashboardScreen.classList.add('hidden');
        usernameInput.value = '';
    } catch (e) {
        console.error('Logout failed:', e);
    }
}

// ============== Tab Navigation ==============

function switchTab(tabName) {
    currentTab = tabName;

    tabBtns.forEach(btn => {
        if (btn.dataset.tab === tabName) {
            btn.classList.add('active');
        } else {
            btn.classList.remove('active');
        }
    });

    tabPanes.forEach(pane => {
        if (pane.id === `${tabName}Tab`) {
            pane.classList.add('active');
        } else {
            pane.classList.remove('active');
        }
    });

    // Load data when switching tabs
    switch (tabName) {
        case 'myImages':
            loadMyImages();
            break;
        case 'browse':
            loadOnlineUsers();
            break;
        case 'requests':
            loadRequests();
            break;
        case 'viewable':
            loadViewableImages();
            break;
    }
}

// ============== My Images Tab ==============

async function loadMyImages() {
    try {
        const data = await apiCall('/api/my-images');

        if (data.count === 0) {
            myImagesGrid.innerHTML = '<p class="empty-state">No images yet. Upload your first 128×128 image!</p>';
            return;
        }

        myImagesGrid.innerHTML = data.images.map(filename => `
      <div class="image-card" onclick="viewImage('${currentUser}', '${filename}')">
        <img src="/api/image/${currentUser}/${filename}" alt="${filename}" />
        <div class="image-card-info">
          <p>${filename}</p>
        </div>
      </div>
    `).join('');

    } catch (e) {
        myImagesGrid.innerHTML = '<p class="empty-state">Failed to load images</p>';
    }
}

async function uploadImage() {
    const file = imageUpload.files[0];
    if (!file) return;

    // Validate image dimensions
    const img = new Image();
    img.src = URL.createObjectURL(file);

    img.onload = async () => {
        URL.revokeObjectURL(img.src);

        // Image will be resized on server, so just upload
        const formData = new FormData();
        formData.append('image', file);

        try {
            const response = await fetch('/api/upload', {
                method: 'POST',
                body: formData
            });

            if (!response.ok) {
                const error = await response.json();
                throw new Error(error.error || 'Upload failed');
            }

            const data = await response.json();
            alert(`✅ Image uploaded: ${data.filename}`);

            // Reload images
            loadMyImages();
            imageUpload.value = '';

        } catch (e) {
            alert(`❌ Upload failed: ${e.message}`);
        }
    };
}

// ============== Browse Tab ==============

async function loadOnlineUsers() {
    try {
        const data = await apiCall('/api/discover');

        if (data.count === 0) {
            usersList.innerHTML = '<p class="empty-state">No users online</p>';
            return;
        }

        usersList.innerHTML = data.online_clients.map(user => `
      <div class="user-item" id="user-${user.username}">
        <div class="user-header" onclick="toggleUser('${user.username}')">
          <div class="user-info">
            <div class="user-avatar">${user.username[0].toUpperCase()}</div>
            <div>
              <div class="user-name">${user.username}</div>
              <div class="user-status">● Online</div>
            </div>
          </div>
          <span class="expand-icon">▼</span>
        </div>
        <div class="user-images" id="images-${user.username}">
          <div class="image-grid">
            <p class="empty-state">Loading...</p>
          </div>
        </div>
      </div>
    `).join('');

    } catch (e) {
        usersList.innerHTML = '<p class="empty-state">Failed to load users</p>';
    }
}

async function toggleUser(username) {
    const userItem = document.getElementById(`user-${username}`);
    const imagesContainer = document.getElementById(`images-${username}`);

    if (userItem.classList.contains('expanded')) {
        userItem.classList.remove('expanded');
        return;
    }

    userItem.classList.add('expanded');

    // Load user's images
    try {
        const data = await apiCall(`/api/user-images/${username}`);

        if (data.count === 0) {
            imagesContainer.innerHTML = '<p class="empty-state">No images</p>';
            return;
        }

        imagesContainer.innerHTML = `
      <div class="image-grid">
        ${data.images.map(filename => `
          <div class="image-card" onclick="requestImageAccess('${username}', '${filename}')">
            <img src="/api/image/${username}/${filename}" alt="${filename}" />
            <div class="image-card-info">
              <p>${filename}</p>
            </div>
          </div>
        `).join('')}
      </div>
    `;

    } catch (e) {
        imagesContainer.innerHTML = '<p class="empty-state">Failed to load images</p>';
    }
}

// ============== Request Access ==============

function requestImageAccess(username, filename) {
    requestUsername.textContent = username;
    requestImageName.textContent = filename;
    requestModal.classList.remove('hidden');
}

async function sendViewRequest() {
    const username = requestUsername.textContent;
    const image = requestImageName.textContent;

    try {
        await apiCall('/api/request-view', {
            method: 'POST',
            body: JSON.stringify({ username, image })
        });

        alert('✅ Request sent!');
        requestModal.classList.add('hidden');

    } catch (e) {
        alert('❌ Failed to send request');
    }
}

// ============== Requests Tab ==============

async function loadRequests() {
    try {
        const data = await apiCall('/api/requests');

        if (data.requests.length === 0) {
            requestsList.innerHTML = '<p class="empty-state">No pending requests</p>';
            document.getElementById('requestCount').textContent = '0';
            return;
        }

        document.getElementById('requestCount').textContent = data.requests.length;

        requestsList.innerHTML = data.requests.map((req, idx) => `
      <div class="request-card">
        <div class="request-info">
          <h3>Request from: ${req.from}</h3>
          <p>Image: ${req.image}</p>
        </div>
        <div class="request-actions">
          <button class="btn-primary" onclick="approveRequest(${idx})">Approve</button>
          <button class="btn-secondary" onclick="rejectRequest(${idx})">Reject</button>
        </div>
      </div>
    `).join('');

    } catch (e) {
        requestsList.innerHTML = '<p class="empty-state">Failed to load requests</p>';
    }
}

function approveRequest(idx) {
    // TODO: Load request data
    approveUsername.textContent = 'user'; // Replace with actual
    approveImageName.textContent = 'image.png'; // Replace with actual
    approveModal.classList.remove('hidden');
}

function rejectRequest(idx) {
    if (confirm('Reject this request?')) {
        // TODO: Implement rejection
        alert('Request rejected');
        loadRequests();
    }
}

async function approveWithCover() {
    const coverFile = coverImageInput.files[0];
    if (!coverFile) {
        alert('Please select a cover image');
        return;
    }

    const viewCount = viewCountInput.value;

    // TODO: Implement steganography
    alert('✅ Approval sent (steganography not yet implemented)');
    approveModal.classList.add('hidden');
    loadRequests();
}

// ============== Viewable Images Tab ==============

async function loadViewableImages() {
    try {
        const data = await apiCall('/api/viewable');

        if (data.images.length === 0) {
            viewableList.innerHTML = '<p class="empty-state">No shared images yet</p>';
            document.getElementById('viewableCount').textContent = '0';
            return;
        }

        document.getElementById('viewableCount').textContent = data.images.length;

        viewableList.innerHTML = data.images.map((img, idx) => `
      <div class="viewable-card">
        <div class="viewable-info">
          <h3>${img.image}</h3>
          <p>From: ${img.from}</p>
          <p>Views remaining: <span class="view-count">${img.views}</span></p>
        </div>
        <button class="btn-primary" onclick="viewStegImage(${idx})">VIEW</button>
      </div>
    `).join('');

    } catch (e) {
        viewableList.innerHTML = '<p class="empty-state">Failed to load images</p>';
    }
}

function viewStegImage(idx) {
    // TODO: Implement steganography extraction
    alert('Steganography extraction not yet implemented');
}

// ============== Image Viewer ==============

function viewImage(username, filename) {
    modalImage.src = `/api/image/${username}/${filename}`;
    modalTitle.textContent = filename;
    modalDetails.textContent = `Owner: ${username}`;
    imageModal.classList.remove('hidden');
}

// ============== Event Listeners ==============

const registerBtn = document.getElementById('registerBtn');

loginBtn.addEventListener('click', login);
if (registerBtn) registerBtn.addEventListener('click', register);
logoutBtn.addEventListener('click', logout);

usernameInput.addEventListener('keypress', (e) => {
    if (e.key === 'Enter') login();
});

tabBtns.forEach(btn => {
    btn.addEventListener('click', () => switchTab(btn.dataset.tab));
});

imageUpload.addEventListener('change', uploadImage);

refreshUsersBtn.addEventListener('click', loadOnlineUsers);
refreshRequestsBtn.addEventListener('click', loadRequests);
refreshViewableBtn.addEventListener('click', loadViewableImages);

sendRequestBtn.addEventListener('click', sendViewRequest);
approveBtn.addEventListener('click', approveWithCover);

// Modal close buttons
document.querySelectorAll('.modal-close').forEach(btn => {
    btn.addEventListener('click', () => {
        btn.closest('.modal').classList.add('hidden');
    });
});

// Close modals on outside click
[imageModal, requestModal, approveModal].forEach(modal => {
    modal.addEventListener('click', (e) => {
        if (e.target === modal) {
            modal.classList.add('hidden');
        }
    });
});

// ============== Initialize ==============

console.log('🚀 Cloud Steg User Dashboard loaded');
