import React from 'react';
import { useParams } from 'react-router-dom';
import MockEditor from '../../components/MockEditor/MockEditor';

const MockEditPage: React.FC = () => {
  const { id } = useParams<{ id: string }>();
  return <MockEditor mode="edit" mockId={id} />;
};

export default MockEditPage;
