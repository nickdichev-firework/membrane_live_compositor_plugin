#include "RectVAO.h"

/**
 * @brief Construct a new RectVAO object
 * 
 * @param vertices A vector of vertex positions for this rectangle's corners 
 *                 and their mappings into the texture space. This should be 
 *                 exactly 5 floats long per vertex (3 values between -1 and 1
 *                 representing x, y and z coordinates for the vertex and two
 *                 values between 0 and 1 representing where this vertex is in 
 *                 texture space)
 * @param indices A vector of unsigned ints representing the elements buffer
 */
RectVAO::RectVAO(const std::vector<float> &vertices,
                 const std::vector<unsigned int> &indices)
    : m_id(0), m_vertex_buffer_id(0), m_elements_buffer_id(0) {
  glGenVertexArrays(1, &m_id);
  glBindVertexArray(m_id);

  glGenBuffers(1, &m_vertex_buffer_id);
  glBindBuffer(GL_ARRAY_BUFFER, m_vertex_buffer_id);
  glBufferData(GL_ARRAY_BUFFER, vertices.size() * sizeof(float),
               vertices.data(), GL_STATIC_DRAW);

  glVertexAttribPointer(0, 3, GL_FLOAT, GL_FALSE, 5 * sizeof(float), (void *)0);
  glEnableVertexAttribArray(0);

  glVertexAttribPointer(1, 2, GL_FLOAT, GL_FALSE, 5 * sizeof(float),
                        (void *)(3 * sizeof(float)));
  glEnableVertexAttribArray(1);

  glGenBuffers(1, &m_elements_buffer_id);
  glBindBuffer(GL_ELEMENT_ARRAY_BUFFER, m_elements_buffer_id);
  glBufferData(GL_ELEMENT_ARRAY_BUFFER, indices.size() * sizeof(float),
               indices.data(), GL_STATIC_DRAW);

  m_indices_amount = indices.size();
}

RectVAO::~RectVAO() {
  if (m_vertex_buffer_id != 0) glDeleteBuffers(1, &m_vertex_buffer_id);
  if (m_elements_buffer_id != 0) glDeleteBuffers(1, &m_elements_buffer_id);
  if (m_id != 0) glDeleteVertexArrays(1, &m_id);
}

/**
 * @brief Bind this VAO for drawing
 * 
 */
void RectVAO::bind() const { glBindVertexArray(m_id); }

/**
 * @brief Draw this VAO
 * 
 */
void RectVAO::draw() const {
  bind();
  glDrawElements(GL_TRIANGLES, m_indices_amount, GL_UNSIGNED_INT, nullptr);
}

RectVAO::RectVAO(RectVAO &&other) noexcept
    : m_id(other.m_id),
      m_vertex_buffer_id(other.m_vertex_buffer_id),
      m_elements_buffer_id(other.m_elements_buffer_id),
      m_indices_amount(other.m_indices_amount) {
  other.m_id = 0;
  other.m_elements_buffer_id = 0;
  other.m_vertex_buffer_id = 0;
}

RectVAO &RectVAO::operator=(RectVAO &&other) noexcept {
  if (this != &other) {
    this->m_id = other.m_id;
    other.m_id = 0;
    this->m_vertex_buffer_id = other.m_vertex_buffer_id;
    other.m_vertex_buffer_id = 0;
    this->m_elements_buffer_id = other.m_elements_buffer_id;
    other.m_elements_buffer_id = 0;
    this->m_indices_amount = other.m_indices_amount;
  }
  return *this;
}